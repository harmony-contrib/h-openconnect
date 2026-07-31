#!/usr/bin/env python3
"""Configure disposable HarmonyOS signing material for CI.

Set OHOS_CI_SIGNING_JSON to a JSON object whose binary values are base64:

  {
    "certBase64": "...",
    "profileBase64": "...",
    "storeBase64": "...",
    "keyAlias": "debugKey",
    "keyPassword": "...",
    "storePassword": "...",
    "signAlg": "SHA256withECDSA"
  }

When the variable is absent, the script only validates the signing material
already referenced by build-profile.json5. This keeps a preconfigured
self-hosted runner usable without copying secrets into the repository.
"""

from __future__ import annotations

import base64
import binascii
import json
import os
from pathlib import Path
import sys
from typing import NoReturn


ROOT = Path(__file__).resolve().parent.parent
BUILD_PROFILE = ROOT / "build-profile.json5"
MATERIAL_DIR = Path(
    os.environ.get("OHOS_CI_SIGNING_DIR", os.environ.get("RUNNER_TEMP", "/tmp"))
) / "hopenconnect-signing"


def fail(message: str) -> NoReturn:
    print(f"configure-ci-signing: {message}", file=sys.stderr)
    raise SystemExit(1)


def load_build_profile() -> dict:
    try:
        return json.loads(BUILD_PROFILE.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read {BUILD_PROFILE}: {error}")


def signing_material(profile: dict) -> dict:
    try:
        configs = profile["app"]["signingConfigs"]
        config = next(item for item in configs if item.get("name") == "default")
        return config["material"]
    except (KeyError, StopIteration, TypeError) as error:
        fail(f"default signing config is missing: {error}")


def decode_file(value: str, path: Path) -> None:
    try:
        content = base64.b64decode(value, validate=True)
    except (ValueError, binascii.Error) as error:
        fail(f"invalid base64 for {path.name}: {error}")
    if not content:
        fail(f"decoded signing file is empty: {path.name}")
    path.write_bytes(content)
    path.chmod(0o600)


def validate_files(material: dict) -> None:
    for key in ("certpath", "profile", "storeFile"):
        value = material.get(key)
        if not isinstance(value, str) or not Path(value).is_file():
            fail(f"signing material {key} is unavailable: {value!r}")


def main() -> None:
    profile = load_build_profile()
    material = signing_material(profile)
    encoded = os.environ.get("OHOS_CI_SIGNING_JSON", "").strip()
    if not encoded:
        validate_files(material)
        print("using signing material configured by the self-hosted runner")
        return

    try:
        secret = json.loads(encoded)
    except json.JSONDecodeError as error:
        fail(f"OHOS_CI_SIGNING_JSON is invalid JSON: {error}")
    if not isinstance(secret, dict):
        fail("OHOS_CI_SIGNING_JSON must be a JSON object")

    required = (
        "certBase64",
        "profileBase64",
        "storeBase64",
        "keyAlias",
        "keyPassword",
        "storePassword",
    )
    missing = [key for key in required if not isinstance(secret.get(key), str) or not secret[key]]
    if missing:
        fail(f"OHOS_CI_SIGNING_JSON is missing: {', '.join(missing)}")

    MATERIAL_DIR.mkdir(parents=True, exist_ok=True)
    MATERIAL_DIR.chmod(0o700)
    cert_path = MATERIAL_DIR / "ci-signing.cer"
    profile_path = MATERIAL_DIR / "ci-signing.p7b"
    store_path = MATERIAL_DIR / "ci-signing.p12"
    decode_file(secret["certBase64"], cert_path)
    decode_file(secret["profileBase64"], profile_path)
    decode_file(secret["storeBase64"], store_path)

    material.update(
        {
            "certpath": str(cert_path),
            "profile": str(profile_path),
            "storeFile": str(store_path),
            "keyAlias": secret["keyAlias"],
            "keyPassword": secret["keyPassword"],
            "storePassword": secret["storePassword"],
            "signAlg": secret.get("signAlg", "SHA256withECDSA"),
        }
    )
    BUILD_PROFILE.write_text(json.dumps(profile, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print("configured disposable HarmonyOS signing material")


if __name__ == "__main__":
    main()
