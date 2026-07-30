#!/usr/bin/env sh
# Device launch smoke for H-OpenConnect.
#
# Connection scenarios are driven through the real UI. Production abilities do
# not accept credentials, trust overrides, or auto-connect commands in Want
# parameters, and the runtime does not emit test-only marker files.
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
HDC="${HDC:-hdc}"
BUNDLE_NAME="${BUNDLE_NAME:-com.richerfu.h_openconnect}"
ABILITY_NAME="${ABILITY_NAME:-EntryAbility}"
HAP_PATH="${HAP_PATH:-$ROOT_DIR/entry/build/default/outputs/default/entry-default-unsigned.hap}"
LOG_DIR="${LOG_DIR:-$ROOT_DIR/smoke-logs}"
HILOG_SECONDS="${HILOG_SECONDS:-8}"
HDC_TARGET="${HDC_TARGET:-}"
RUN_BUILD=1

usage() {
  cat <<USAGE
Usage: scripts/e2e-device.sh [options]

Build/install H-OpenConnect, launch EntryAbility without test parameters, and
verify that the production shell initializes.

Options:
  --no-build          Skip ohrs/hvigor rebuild
  --hap PATH          Install this HAP
  --target KEY        hdc -t KEY
  --hilog-seconds N   Capture window (default 8)
  -h, --help
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --no-build) RUN_BUILD=0; shift ;;
    --hap) HAP_PATH="${2:?}"; shift 2 ;;
    --target) HDC_TARGET="${2:?}"; shift 2 ;;
    --hilog-seconds) HILOG_SECONDS="${2:?}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown arg: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [ -n "$HDC_TARGET" ]; then
  HDC_CMD() { "$HDC" -t "$HDC_TARGET" "$@"; }
else
  HDC_CMD() { "$HDC" "$@"; }
fi

mkdir -p "$LOG_DIR"
STAMP="$(date +%Y%m%d-%H%M%S)"
LOG_FILE="$LOG_DIR/device-smoke-$STAMP.log"

if [ "$RUN_BUILD" -eq 1 ]; then
  (cd "$ROOT_DIR" && ./scripts/package-hap.sh)
fi

if [ ! -f "$HAP_PATH" ]; then
  echo "HAP missing: $HAP_PATH" >&2
  exit 1
fi

HDC_CMD shell aa force-stop "$BUNDLE_NAME" >/dev/null 2>&1 || true
HDC_CMD install -r "$HAP_PATH"
HDC_CMD shell aa start -a "$ABILITY_NAME" -b "$BUNDLE_NAME"

{
  HDC_CMD shell "hilog -x" || true
  sleep "$HILOG_SECONDS"
  HDC_CMD shell "hilog -x" || true
} >"$LOG_FILE" 2>&1 || true

if ! grep -E "HOpenConnectEntry|configured native home|registered native platform callbacks" \
  "$LOG_FILE" >/dev/null 2>&1; then
  echo "device smoke failed. Log: $LOG_FILE" >&2
  tail -n 80 "$LOG_FILE" >&2 || true
  exit 1
fi

echo "device launch smoke OK"
echo "log: $LOG_FILE"
