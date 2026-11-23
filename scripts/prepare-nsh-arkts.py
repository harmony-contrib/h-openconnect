#!/usr/bin/env python3
"""Stage @nsh/harmonyos ArkTS sources inside target/ohos/arkts-deps/nsh without modifying sibling checkouts.

Also re-exports upstream `NativeAuthSession` and `SystemReply` from the staged
package entry so the app host consumes upstream's web sign-in sheet directly
without duplicating any WebView auth UI.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

from nsh_build_inputs import resolve_inputs

ROOT = Path(__file__).resolve().parents[1]
inputs = resolve_inputs()
harmonyos_root = Path(inputs['harmonyosRoot'])
nsh_arkts = harmonyos_root / 'crates/nsh-platform-harmonyos/arkts'
destination = ROOT / 'target/ohos/arkts-deps'
target = destination / 'nsh'

if not (nsh_arkts / 'oh-package.json5').is_file():
    raise SystemExit(f'Matching @nsh/harmonyos ArkTS package is unavailable: {nsh_arkts}')

destination.mkdir(parents=True, exist_ok=True)
for stale in destination.iterdir():
    if stale.name != 'nsh':
        shutil.rmtree(stale) if stale.is_dir() else stale.unlink()

if target.exists():
    shutil.rmtree(target)

shutil.copytree(
    nsh_arkts,
    target,
    ignore=shutil.ignore_patterns(
        'build', '.hvigor', '.cxx', 'oh_modules', 'oh-package-lock.json5', 'BuildProfile.ets'
    ),
)

manifest = target / 'oh-package.json5'
manifest.write_text(
    manifest.read_text().replace(
        '"libnsh_platform_harmonyos.so": "file:./src/main/cpp/types/libnsh_platform_harmonyos"',
        '"libnsh_platform_harmonyos.so": "file:../../../../entry/src/main/cpp/types/libnsh_platform_harmonyos"',
    )
)

# Re-export upstream NativeAuthSession & SystemReply from the staged @nsh/harmonyos index.
index_file = target / 'index.ets'
index_text = index_file.read_text()
extra_exports = [
    'export { NativeAuthSession } from "./src/main/ets/NativeAuthSession";',
    'export { SystemReply } from "./src/main/ets/NativeSystem";',
]
for line in extra_exports:
    if line not in index_text:
        if not index_text.endswith('\n'):
            index_text += '\n'
        index_text += line + '\n'
index_file.write_text(index_text)

original_hash, staged_hash = hashlib.sha256(), hashlib.sha256()
files = {}
for staged in sorted(item for item in target.rglob('*') if item.is_file()):
    relative = staged.relative_to(target).as_posix()
    original, compiled = (nsh_arkts / relative).read_bytes(), staged.read_bytes()
    original_hash.update(relative.encode() + b'\0' + original + b'\0')
    staged_hash.update(relative.encode() + b'\0' + compiled + b'\0')
    files[relative] = {
        'sourceSha256': hashlib.sha256(original).hexdigest(),
        'stagedSha256': hashlib.sha256(compiled).hexdigest(),
    }

studio = Path(os.environ['DEVECO_STUDIO_HOME'])
parser = studio / 'tools/hvigor/hvigor-ohos-plugin/node_modules/json5'
package = json.loads(subprocess.check_output([
    str(studio / 'tools/node/bin/node'), '-e',
    'const fs=require("fs"),j=require(process.argv[1]);process.stdout.write(JSON.stringify(j.parse(fs.readFileSync(process.argv[2],"utf8"))))',
    str(parser), str(manifest),
], text=True, timeout=30))

sidecar = {
    'format': 'HOpenConnect-matching-ArkTS-sources-v1',
    'nsh': inputs['nsh'],
    'harmonyos': inputs['harmonyos'],
    'packages': [{
        'name': package['name'],
        'version': package['version'],
        'sourceRoot': str(nsh_arkts),
        'stagedRoot': str(target.relative_to(ROOT)),
        'sourceSha256': original_hash.hexdigest(),
        'stagedSha256': staged_hash.hexdigest(),
        'fileCount': len(files),
        'files': files,
    }],
}
(ROOT / 'target/ohos/arkts-source-inputs.json').write_text(json.dumps(sidecar, indent=2) + '\n')
print(destination)
