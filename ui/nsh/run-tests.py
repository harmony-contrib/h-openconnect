#!/usr/bin/env python3
"""Run NSH type-check, production bundle sanity check, and headless UI acceptance tests."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")


def run_step(name: str, cmd: list[str], log_dir: Path, timeout: float) -> str:
    started = time.monotonic()
    print(f"Running NSH {name}...", flush=True)
    log_path = log_dir / f"{name}.log"
    result = subprocess.run(
        cmd,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=timeout,
        check=False,
    )
    output = ANSI.sub("", result.stdout)
    log_path.write_text(output, encoding="utf-8")
    if result.returncode != 0:
        print(f"FAIL: {name} exited with status {result.returncode}; log: {log_path}", file=sys.stderr)
        print("\n".join(output.splitlines()[-30:]), file=sys.stderr)
        sys.exit(1)
    print(f"PASS: {name} ({time.monotonic() - started:.2f}s)", flush=True)
    return output


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    default_nsh = os.environ.get("NSH_BIN")
    if not default_nsh:
        staged_nsh = ROOT / "dist/nsh-sdk/release/nsh"
        default_nsh = str(staged_nsh) if staged_nsh.is_file() else (shutil.which("nsh") or str(staged_nsh))
    parser.add_argument(
        "--nsh-bin",
        default=default_nsh,
        help="Path to the nsh CLI binary",
    )
    parser.add_argument(
        "--log-dir",
        type=Path,
        default=ROOT / "target/nsh-ui-tests",
        help="Directory to write test logs and build verification artifacts",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=90,
        help="Per-command timeout in seconds",
    )
    options = parser.parse_args()

    binary = Path(options.nsh_bin).expanduser()
    if not binary.is_absolute():
        binary = ROOT / binary
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error(f"NSH executable is unavailable: {binary}; build the SDK or set NSH_BIN")

    log_dir = options.log_dir.expanduser().resolve()
    log_dir.mkdir(parents=True, exist_ok=True)

    run_step("check", [str(binary), "check", "ui/nsh"], log_dir, options.timeout)

    bundle_dir = log_dir / "production-entry"
    run_step(
        "production-entry",
        [str(binary), "build", "ui/nsh", "-o", str(bundle_dir)],
        log_dir,
        options.timeout,
    )
    bundle_js = bundle_dir / "bundle.js"
    bundle_nshc = bundle_dir / "bundle.nshc"
    if not bundle_js.is_file():
        print(f"FAIL: missing bundle output at {bundle_js}", file=sys.stderr)
        return 1
    if not bundle_nshc.is_file() or bundle_nshc.stat().st_size == 0:
        print(f"FAIL: missing bytecode bundle output at {bundle_nshc}", file=sys.stderr)
        return 1
    artifact = bundle_js.read_text(encoding="utf-8")
    if "new CoreClient" not in artifact:
        print("FAIL: production bundle does not construct CoreClient", file=sys.stderr)
        return 1
    if "data:image/svg+xml" in artifact:
        print("FAIL: OHOS bitmap decoding cannot render SVG data URLs", file=sys.stderr)
        return 1

    output = run_step(
        "acceptance",
        [str(binary), "test", "ui/nsh/tests/app.test.ts"],
        log_dir,
        options.timeout,
    )
    if "0 failed" not in output:
        print("FAIL: acceptance tests did not report 0 failed", file=sys.stderr)
        print(output, file=sys.stderr)
        return 1

    print(f"PASS: all NSH checks and acceptance tests passed; logs: {log_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
