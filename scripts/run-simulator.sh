#!/usr/bin/env sh
set -eu

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLE_NAME="${BUNDLE_NAME:-com.richerfu.h_openconnect}"
ABILITY="${ABILITY:-EntryAbility}"
# Simulator accepts the unsigned HAP produced by PackageHap. Signed HAP requires
# a bundleName-matching debug profile from DevEco (File > Project Structure >
# Signing Configs > Fix).
SIGNED_HAP="${SIGNED_HAP:-$ROOT_DIR/entry/build/default/outputs/default/entry-default-signed.hap}"
UNSIGNED_HAP="${UNSIGNED_HAP:-$ROOT_DIR/entry/build/default/outputs/default/entry-default-unsigned.hap}"
HAP_PATH="${HAP_PATH:-}"
HDC="${HDC:-hdc}"
HDC_TARGET="${HDC_TARGET:-}"
BUILD_FIRST="${BUILD_FIRST:-1}"

if [ -n "$HDC_TARGET" ]; then
  HDC_CMD="$HDC -t $HDC_TARGET"
else
  HDC_CMD="$HDC"
fi

if [ "$BUILD_FIRST" = "1" ]; then
  NATIVE_PROFILE="${NATIVE_PROFILE:-release}" HAP_BUILD_MODE="${HAP_BUILD_MODE:-release}" \
    "$ROOT_DIR/scripts/package-hap.sh"
fi

if [ -z "$HAP_PATH" ]; then
  if [ -f "$SIGNED_HAP" ]; then
    HAP_PATH="$SIGNED_HAP"
  else
    HAP_PATH="$UNSIGNED_HAP"
  fi
fi
if [ ! -f "$HAP_PATH" ]; then
  echo "HAP not found: $HAP_PATH" >&2
  exit 1
fi

echo "Installing $HAP_PATH"
$HDC_CMD shell aa force-stop "$BUNDLE_NAME" >/dev/null 2>&1 || true
$HDC_CMD uninstall "$BUNDLE_NAME" >/dev/null 2>&1 || true
$HDC_CMD install -r "$HAP_PATH"
$HDC_CMD shell aa start -a "$ABILITY" -b "$BUNDLE_NAME"
echo "Started $BUNDLE_NAME/$ABILITY"
