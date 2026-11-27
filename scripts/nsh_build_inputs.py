"""Record the HarmonyOS NSH backend and CLI build inputs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
RUNNER_DIR = ROOT / 'scripts/nsh-harmonyos-runner'
RUNNER_MANIFEST = RUNNER_DIR / 'Cargo.toml'
OHOS_TARGET = 'aarch64-unknown-linux-ohos'


def _arkts_dir(harmonyos_root: Path) -> Path:
    candidate = harmonyos_root / 'arkts'
    if (candidate / 'oh-package.json5').is_file():
        return candidate
    legacy = harmonyos_root / 'crates/nsh-platform-harmonyos/arkts'
    if (legacy / 'oh-package.json5').is_file():
        return legacy
    raise ValueError(f'Matching @nsh/harmonyos ArkTS package is unavailable under {harmonyos_root}')


def _hash_tree(root: Path, base: Path | None = None) -> tuple[str, int]:
    base = base or root
    generated = {
        'target', 'dist', 'build', 'oh_modules', 'node_modules',
        '.hvigor', '.cxx', '.nsh', '__pycache__', '.git',
    }
    files = []
    for directory, children, names in os.walk(root):
        current = Path(directory)
        children[:] = [name for name in children if name not in generated]
        files.extend(current / name for name in names if name != '.DS_Store')
    digest = hashlib.sha256(b'HOpenConnect-NSH-OHOS-source-v2\0')
    for filename in sorted(files):
        relative = os.fsencode(filename.relative_to(base))
        digest.update(relative + b'\0')
        if filename.is_symlink():
            digest.update(b'L' + os.fsencode(os.readlink(filename)))
        else:
            with filename.open('rb') as stream:
                for chunk in iter(lambda: stream.read(1 << 20), b''):
                    digest.update(chunk)
        digest.update(b'\0')
    return digest.hexdigest(), len(files)


def _git_identity(path: Path) -> dict:
    git = ['git', '-c', 'core.fsmonitor=false', '-C', str(path)]
    revision = subprocess.check_output([*git, 'rev-parse', 'HEAD'], text=True, timeout=30).strip()
    status = subprocess.check_output(
        [*git, 'status', '--porcelain=v1', '--untracked-files=normal'], timeout=30
    )
    dirty = bool(status.splitlines())
    digest = hashlib.sha256(b'HOpenConnect-NSH-worktree-v2\0' + revision.encode() + b'\0')
    digest.update(
        subprocess.check_output([*git, 'diff', '--no-ext-diff', '--no-textconv', 'HEAD', '--binary'], timeout=30)
    )
    arkts_sha, arkts_count = _hash_tree(_arkts_dir(path), path)
    digest.update(b'\0NSH-OHOS-arkts\0' + arkts_sha.encode())
    return {
        'sourceKind': 'local-checkout',
        'revision': revision,
        'dirty': dirty,
        'arktsSha256': arkts_sha,
        'arktsFileCount': arkts_count,
        'worktreeSha256': digest.hexdigest(),
    }


def _crates_io_inputs() -> dict:
    command = [
        'cargo', 'metadata',
        '--manifest-path', str(RUNNER_MANIFEST),
        '--format-version', '1',
        '--filter-platform', OHOS_TARGET,
    ]
    if (RUNNER_DIR / 'Cargo.lock').is_file():
        command.append('--locked')
    metadata = json.loads(subprocess.check_output(command, text=True, timeout=180))
    packages = {pkg['name']: pkg for pkg in metadata['packages']}

    harmonyos_pkg = packages.get('nsh-platform-harmonyos')
    if not harmonyos_pkg:
        raise ValueError('nsh-platform-harmonyos was not found in runner Cargo metadata')
    nsh_core_pkg = packages.get('nsh-core')
    if not nsh_core_pkg:
        raise ValueError('nsh-core was not found in runner Cargo metadata')

    harmonyos_root = Path(harmonyos_pkg['manifest_path']).resolve(strict=True).parent
    arkts_dir = _arkts_dir(harmonyos_root)
    arkts_sha, arkts_count = _hash_tree(arkts_dir, harmonyos_root)

    runner_digest = hashlib.sha256(b'HOpenConnect-NSH-runner-v2\0')
    for name in ('Cargo.toml', 'Cargo.lock', 'lib.rs', 'build.rs'):
        file_path = RUNNER_DIR / name
        if file_path.is_file():
            runner_digest.update(name.encode() + b'\0' + file_path.read_bytes() + b'\0')
    runner_sha = runner_digest.hexdigest()

    combined = hashlib.sha256(
        b'HOpenConnect-NSH-crates-io-v2\0'
        + harmonyos_pkg['version'].encode() + b'\0'
        + nsh_core_pkg['version'].encode() + b'\0'
        + runner_sha.encode() + b'\0'
        + arkts_sha.encode()
    ).hexdigest()

    resolved_crates = {
        name: packages[name]['version']
        for name in (
            'nsh-platform-harmonyos',
            'nsh-core',
            'nsh-engine',
            'nsh-base',
            'ohos-display-binding',
            'ohos-arkui-binding',
            'ohos-enum-derive',
        )
        if name in packages
    }

    return {
        'harmonyosRoot': str(harmonyos_root),
        'harmonyosVersion': harmonyos_pkg['version'],
        'nshCoreVersion': nsh_core_pkg['version'],
        'nshRevision': f"nsh-platform-harmonyos-{harmonyos_pkg['version']}",
        'hostModel': 'native-ui-context',
        'harmonyos': {
            'sourceKind': 'crates-io',
            'version': harmonyos_pkg['version'],
            'source': harmonyos_pkg.get('source'),
            'runnerSha256': runner_sha,
            'arktsSha256': arkts_sha,
            'arktsFileCount': arkts_count,
            'worktreeSha256': combined,
            'resolvedCrates': resolved_crates,
        },
    }


def resolve_inputs() -> dict:
    local_override = os.environ.get('NSH_HARMONYOS_ROOT', '').strip()
    if local_override:
        harmonyos_root = Path(local_override).resolve(strict=True)
        identity = _git_identity(harmonyos_root)
        version = 'local'
        cargo_toml = harmonyos_root / 'Cargo.toml'
        if cargo_toml.is_file():
            import tomllib
            manifest = tomllib.loads(cargo_toml.read_text())
            version = manifest.get('package', {}).get('version', version)
        return {
            'harmonyosRoot': str(harmonyos_root),
            'harmonyosVersion': version,
            'nshCoreVersion': '0.7.2',
            'nshRevision': identity['revision'] + ('+dirty' if identity['dirty'] else ''),
            'hostModel': 'native-ui-context',
            'harmonyos': identity,
        }
    return _crates_io_inputs()


def verify_source_identity(recorded: dict, current: dict) -> None:
    """Reject missing fingerprints and any changes to the backend inputs."""
    if not recorded.get('harmonyos', {}).get('worktreeSha256'):
        raise ValueError('NSH SDK has no recorded backend digest; rebuild before reuse')
    if {key: recorded.get(key) for key in current} != current:
        raise ValueError('NSH SDK inputs changed; rebuild before reuse')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        '--field',
        choices=('harmonyosRoot', 'harmonyosVersion', 'nshCoreVersion', 'nshRevision'),
    )
    args = parser.parse_args()
    values = resolve_inputs()
    print(values[args.field] if args.field else json.dumps(values, indent=2))
