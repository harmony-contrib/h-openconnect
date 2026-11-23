"""Record the pinned local NSH SDK and HarmonyOS backend build inputs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def _pinned_revision(filename):
    revision = (ROOT / filename).read_text().strip()
    if len(revision) != 40 or any(character not in '0123456789abcdef' for character in revision):
        raise ValueError(f'{filename} must contain one complete commit SHA')
    return revision


def default_nsh_root():
    """Locate the immutable local NSH checkout selected for this application."""
    return ROOT / 'target/nsh-checkouts' / _pinned_revision('scripts/nsh-revision')


def default_harmonyos_root():
    """Locate the immutable local nsh-platform-harmonyos checkout selected for this application."""
    return ROOT / 'target/nsh-harmonyos-checkouts' / _pinned_revision('scripts/nsh-harmonyos-revision')


def resolve_inputs():
    selected = os.environ.get('NSH_ROOT')
    nsh = (Path(selected) if selected else default_nsh_root()).resolve(strict=True)
    harmonyos_selected = os.environ.get('NSH_HARMONYOS_ROOT')
    harmonyos = (Path(harmonyos_selected) if harmonyos_selected else default_harmonyos_root()).resolve(strict=True)

    manifest = tomllib.loads((nsh / 'Cargo.toml').read_text())
    harmonyos_manifest = tomllib.loads((harmonyos / 'Cargo.toml').read_text())

    def identity(path, sdk_subdir=None):
        git = ['git', '-c', 'core.fsmonitor=false', '-C', str(path)]
        revision = subprocess.check_output([*git, 'rev-parse', 'HEAD'], text=True, timeout=30).strip()
        status = subprocess.check_output([*git, 'status', '--porcelain=v1', '--untracked-files=normal'], timeout=30)
        dirty = bool(status.splitlines())
        digest = hashlib.sha256(b'HOpenConnect-NSH-worktree-v1\0' + revision.encode() + b'\0')
        digest.update(subprocess.check_output([*git, 'diff', '--no-ext-diff', '--no-textconv', 'HEAD', '--binary'], timeout=30))
        untracked = subprocess.check_output([*git, 'ls-files', '-z', '--others', '--exclude-standard'], timeout=30)
        for relative in sorted(filter(None, untracked.split(b'\0'))):
            digest.update(len(relative).to_bytes(8, 'little') + relative)
            filename = path / os.fsdecode(relative)
            if filename.is_symlink():
                content = os.fsencode(os.readlink(filename))
                digest.update(b'L' + len(content).to_bytes(8, 'little') + content)
            else:
                digest.update(b'F' + filename.stat().st_size.to_bytes(8, 'little'))
                with filename.open('rb') as stream:
                    for chunk in iter(lambda: stream.read(1 << 20), b''):
                        digest.update(chunk)
        result = {'revision': revision, 'dirty': dirty, 'worktreeSha256': ''}
        if sdk_subdir:
            port = path / sdk_subdir
            files = []
            generated = {'target', 'dist', 'build', 'oh_modules', 'node_modules', '.hvigor', '.cxx', '.nsh', '__pycache__'}
            for directory, children, names in os.walk(port):
                current = Path(directory)
                if current in (port, port / 'arkts'):
                    children[:] = [name for name in children if name not in generated]
                children[:] = [name for name in children if name != '.git']
                files.extend(current / name for name in names if name != '.DS_Store')
            sdk_digest = hashlib.sha256(b'HOpenConnect-NSH-OHOS-source-v1\0')
            ignored = set(subprocess.check_output(
                [*git, 'ls-files', '-z', '--others', '--ignored', '--exclude-standard', '--', sdk_subdir],
                timeout=30,
            ).split(b'\0'))
            for filename in sorted(files):
                relative = os.fsencode(filename.relative_to(path))
                sdk_digest.update(relative + b'\0')
                if filename.is_symlink():
                    sdk_digest.update(b'L' + os.fsencode(os.readlink(filename)))
                else:
                    with filename.open('rb') as stream:
                        for chunk in iter(lambda: stream.read(1 << 20), b''):
                            sdk_digest.update(chunk)
                sdk_digest.update(b'\0')
                if relative in ignored:
                    result['dirty'] = True
            result.update(sdkSourceSha256=sdk_digest.hexdigest(), sdkSourceFileCount=len(files))
            digest.update(b'\0NSH-OHOS-source-inputs\0' + sdk_digest.digest())
        result['worktreeSha256'] = digest.hexdigest()
        return result

    nsh_identity = identity(nsh)
    harmonyos_identity = identity(harmonyos, sdk_subdir='crates/nsh-platform-harmonyos')
    result = {
        'nshRoot': str(nsh),
        'harmonyosRoot': str(harmonyos),
        'nsh': nsh_identity,
        'harmonyos': harmonyos_identity,
        'nshRevision': harmonyos_identity['revision'] + ('+dirty' if harmonyos_identity['dirty'] else ''),
        'hostModel': 'native-ui-context',
    }

    path_dependencies = {}
    for workspace_root, workspace_manifest in ((nsh, manifest), (harmonyos, harmonyos_manifest)):
        for packages in workspace_manifest.get('patch', {}).values():
            for name, options in packages.items():
                if not isinstance(options, dict) or 'path' not in options:
                    continue
                package = (workspace_root / options['path']).resolve(strict=True)
                root = Path(subprocess.check_output(
                    ['git', '-c', 'core.fsmonitor=false', '-C', str(package), 'rev-parse', '--show-toplevel'],
                    text=True, timeout=30,
                ).strip()).resolve(strict=True)
                key = str(root)
                if key not in path_dependencies:
                    path_dependencies[key] = {'root': key, 'identity': identity(root), 'packages': {}}
                path_dependencies[key]['packages'][name] = str(package)
    if path_dependencies:
        result['pathDependencies'] = [path_dependencies[key] for key in sorted(path_dependencies)]
    return result


def verify_source_identity(recorded, current):
    """Reject missing fingerprints and any changes to any checked-out source tree."""
    for name in ('nsh', 'harmonyos'):
        if not recorded.get(name, {}).get('worktreeSha256'):
            raise ValueError('NSH SDK has no recorded worktree digest; rebuild before reuse')
    if {key: recorded.get(key) for key in current} != current:
        raise ValueError('NSH SDK source roots, revisions or worktree content changed; rebuild before reuse')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--field', choices=('nshRoot', 'harmonyosRoot', 'nshRevision'))
    args = parser.parse_args()
    values = resolve_inputs()
    print(values[args.field] if args.field else json.dumps(values, indent=2))
