#!/usr/bin/env python3
"""Stage pinned local NSH and nsh-platform-harmonyos checkouts without touching sibling repos."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess

from nsh_build_inputs import (
    ROOT,
    default_harmonyos_root,
    default_nsh_root,
    resolve_inputs,
)


class BuildSource:
    def __init__(self):
        if not os.environ.get('NSH_ROOT') or not os.environ.get('NSH_HARMONYOS_ROOT'):
            self.checkout()
        self.inputs = resolve_inputs()
        self.env_var = 'NSH_HARMONYOS_ROOT'
        self.input_key = 'harmonyos'
        self.source = Path(self.inputs['harmonyosRoot'])
        if self.inputs.get('hostModel') != 'native-ui-context':
            raise ValueError('Select the pinned native UIContext SDK')
        recipe = hashlib.sha256(Path(__file__).read_bytes())
        recipe.update((ROOT / 'scripts/nsh_build_inputs.py').read_bytes())
        self.patches = [(path, path.read_bytes()) for path in sorted((ROOT / 'patches/nsh').glob('*.patch'))]
        for path, content in self.patches:
            recipe.update(path.name.encode() + b'\0' + content + b'\0')
        self.recipe = recipe.hexdigest()
        if self.source.parent == ROOT / 'target/nsh-build-source':
            self.stage = self.source
            self.record = self.stage.with_suffix('.json')
            if not self.record.is_file():
                raise ValueError('Incomplete NSH stage; select the original NSH_HARMONYOS_ROOT')
            expected = json.loads(self.record.read_text())
            if expected.get('recipeSha256') != self.recipe or expected['preparedInputs'] != self.inputs:
                raise ValueError('Staged NSH sources changed; select the original NSH_HARMONYOS_ROOT')
            return
        digest = hashlib.sha256(json.dumps(self.inputs, sort_keys=True).encode())
        digest.update(self.recipe.encode())
        self.stage = ROOT / 'target/nsh-build-source' / digest.hexdigest()[:24]
        self.record = self.stage.with_suffix('.json')

    @classmethod
    def checkout(cls):
        """Clone Git objects from sibling repositories into target/, never mutating worktrees."""
        if not os.environ.get('NSH_ROOT'):
            cls.checkout_at(ROOT.parent / 'nsh', default_nsh_root())
        if not os.environ.get('NSH_HARMONYOS_ROOT'):
            cls.checkout_at(ROOT.parent / 'nsh-platform-harmonyos', default_harmonyos_root())

    @staticmethod
    def checkout_at(source, target):
        """Create a clean pinned checkout and reject subsequent tampering."""
        revision = target.name
        git = ['git', '-c', 'core.fsmonitor=false']
        if not target.exists():
            subprocess.run([*git, '-C', str(source), 'cat-file', '-e', revision + '^{commit}'],
                           check=True, timeout=30)
            target.parent.mkdir(parents=True, exist_ok=True)
            subprocess.run([*git, 'clone', '--no-hardlinks', '--no-checkout', '--', str(source), str(target)],
                           check=True, stdout=subprocess.DEVNULL, timeout=180)
            subprocess.run([*git, '-C', str(target), '-c', 'core.hooksPath=/dev/null',
                            'checkout', '--detach', revision],
                           check=True, stdout=subprocess.DEVNULL, timeout=60)
        head = subprocess.check_output([*git, '-C', str(target), 'rev-parse', 'HEAD'],
                                       text=True, timeout=30).strip()
        status = subprocess.check_output([*git, '-C', str(target), 'status', '--porcelain=v1',
                                          '--untracked-files=all'], timeout=30)
        if head != revision or status:
            raise ValueError(f'The pinned source checkout changed: {target}')

    def identity(self, path):
        previous = os.environ.get(self.env_var)
        os.environ[self.env_var] = str(path)
        try:
            return resolve_inputs()
        finally:
            if previous is None:
                os.environ.pop(self.env_var, None)
            else:
                os.environ[self.env_var] = previous

    def prepare(self):
        if self.record.exists():
            expected = json.loads(self.record.read_text())
            if (expected.get('recipeSha256') != self.recipe
                    or self.identity(self.stage) != expected['preparedInputs']):
                raise ValueError('Staged NSH sources changed; remove this generated stage and rebuild')
            return self.stage
        if self.stage.exists():
            shutil.rmtree(self.stage)
        excluded = {'target', 'dist', 'build', 'node_modules', 'oh_modules', '.hvigor',
                    '.cxx', '.nsh', '.DS_Store'}
        if not (self.source / '.git').is_dir() or (self.source / '.git').is_symlink():
            raise ValueError('NSH HarmonyOS source needs an independent .git directory for staging')

        def ignore(directory, names):
            skipped = set(names) & excluded
            for name in names:
                path = Path(directory) / name
                if stat.S_ISSOCK(path.lstat().st_mode) or ('.git' in path.parts and name.endswith('.lock')):
                    skipped.add(name)
            return skipped

        shutil.copytree(self.source, self.stage, ignore=ignore, symlinks=True)
        for key, value in [('core.worktree', str(self.stage)), ('core.fsmonitor', 'false'),
                           ('core.hooksPath', '/dev/null'), ('commit.gpgsign', 'false')]:
            subprocess.run(['git', '-C', str(self.stage), 'config', '--local', key, value],
                           check=True, timeout=30)
        copied = self.identity(self.stage)
        if copied[self.input_key] != self.inputs[self.input_key] or self.identity(self.source) != self.inputs:
            raise ValueError('Local NSH HarmonyOS source changed during staging; retry')
        for path, content in self.patches:
            for arguments in (['--check'], []):
                result = subprocess.run(['git', 'apply', *arguments, '-'], cwd=self.stage,
                                        input=content, capture_output=True, timeout=30)
                if result.returncode:
                    raise ValueError(f'Cannot apply NSH patch {path.name}: '
                                     + result.stderr.decode(errors='replace').strip())
        result = subprocess.run(['cargo', 'metadata', '--locked', '--offline', '--format-version', '1',
                                 '--filter-platform', 'aarch64-unknown-linux-ohos'],
                                cwd=self.stage, capture_output=True, text=True, timeout=180)
        if result.returncode:
            raise ValueError(result.stderr[-4000:])
        packages = json.loads(result.stdout)['packages']
        arkui = [p for p in packages if p['name'] == 'ohos-arkui-binding']
        if len(arkui) != 1:
            raise ValueError('The native SDK graph must contain one ArkUI binding')
        self.record.write_text(json.dumps({
            'originalInputs': self.inputs,
            'recipeSha256': self.recipe,
            'preparedInputs': self.identity(self.stage),
        }, indent=2) + '\n')
        return self.stage


if __name__ == '__main__':
    print(BuildSource().prepare())
