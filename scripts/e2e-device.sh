#!/usr/bin/env sh
# Device E2E for H-AnyConnect (real phone / emulator via hdc).
#
# Mirrors paws harmony-smoke: build → install → start with Want params → greplog.
#
# Examples:
#   # UI + dry-run session (no real VPN headend required)
#   ./scripts/e2e-device.sh
#
#   # Dry-run auto connect with a fake server name
#   ./scripts/e2e-device.sh --server vpn.example.com --auto-connect --dry-run
#
#   # Real headend + OpenConnect mainloop (package-hap defaults to native-anyconnect)
#   ./scripts/e2e-device.sh --server vpn.corp.example --user alice --password secret \
#       --auto-connect --no-dry-run --expect-connected --allow-vpn-unsupported
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
HDC="${HDC:-hdc}"
BUNDLE_NAME="${BUNDLE_NAME:-com.richerfu.hanyconnect}"
ABILITY_NAME="${ABILITY_NAME:-EntryAbility}"
HAP_PATH="${HAP_PATH:-$ROOT_DIR/entry/build/default/outputs/default/entry-default-unsigned.hap}"
LOG_DIR="${LOG_DIR:-$ROOT_DIR/smoke-logs}"
HILOG_SECONDS="${HILOG_SECONDS:-18}"
HDC_TARGET="${HDC_TARGET:-}"
RUN_BUILD=1
FORCE_STOP_APP=1
ALLOW_VPN_UNSUPPORTED=0

SERVER="${SERVER:-}"
NAME="${NAME:-E2E Connection}"
GROUP="${GROUP:-}"
USERNAME="${USERNAME:-}"
PASSWORD="${PASSWORD:-}"
ACCEPT_UNTRUSTED=0
AUTO_CONNECT=0
DRY_RUN=1
EXPECT_CONNECTED=0
EXPECT_FAILURE=0

usage() {
  cat <<USAGE
Usage: scripts/e2e-device.sh [options]

Build/install H-AnyConnect, start EntryAbility with E2E Want parameters,
capture hilog, and assert HAnyConnectE2E markers.

Options:
  --no-build                 Skip ohrs/hvigor rebuild
  --hap PATH                 Install this HAP
  --target KEY               hdc -t KEY
  --hilog-seconds N          Capture window (default 18)
  --server HOST              VPN server for e2e profile
  --name NAME                Profile display name
  --group GROUP              Tunnel group
  --user NAME                Username
  --password SECRET          Password
  --accept-untrusted         Explicitly trust a private/self-signed lab server
  --auto-connect             Trigger connect after applying config
  --dry-run / --no-dry-run   Dry-run session (default on)
  --expect-connected         Require lifecycle connected marker
  --expect-failure           Treat connect failure as success
  --allow-vpn-unsupported    Do not fail if system VPN extension is unavailable
  -h, --help
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --no-build) RUN_BUILD=0; shift ;;
    --hap) HAP_PATH="${2:?}"; shift 2 ;;
    --target) HDC_TARGET="${2:?}"; shift 2 ;;
    --hilog-seconds) HILOG_SECONDS="${2:?}"; shift 2 ;;
    --server) SERVER="${2:?}"; shift 2 ;;
    --name) NAME="${2:?}"; shift 2 ;;
    --group) GROUP="${2:?}"; shift 2 ;;
    --user) USERNAME="${2:?}"; shift 2 ;;
    --password) PASSWORD="${2:?}"; shift 2 ;;
    --accept-untrusted) ACCEPT_UNTRUSTED=1; shift ;;
    --auto-connect) AUTO_CONNECT=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    --no-dry-run) DRY_RUN=0; shift ;;
    --expect-connected) EXPECT_CONNECTED=1; shift ;;
    --expect-failure) EXPECT_FAILURE=1; shift ;;
    --allow-vpn-unsupported) ALLOW_VPN_UNSUPPORTED=1; shift ;;
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
LOG_FILE="$LOG_DIR/e2e-device-$STAMP.log"

if [ "$RUN_BUILD" -eq 1 ]; then
  export OHOS_NDK_HOME="${OHOS_NDK_HOME:-/Applications/DevEco-Studio.app/Contents/sdk/default/openharmony}"
  export DEVECO_SDK_HOME="${DEVECO_SDK_HOME:-/Applications/DevEco-Studio.app/Contents/sdk}"
  export JAVA_HOME="${JAVA_HOME:-/Applications/DevEco-Studio.app/Contents/jbr/Contents/Home}"
  export PATH="$JAVA_HOME/bin:$PATH"
  # Device HAP uses platform backend (no OpenConnect native link).
  (cd "$ROOT_DIR" && ohrs build --arch aarch --release -p hanyconnect_ui)
  mkdir -p "$ROOT_DIR/entry/libs/arm64-v8a"
  cp "$ROOT_DIR/target/aarch64-unknown-linux-ohos/release/libhanyconnect_ui.so" \
    "$ROOT_DIR/entry/libs/arm64-v8a/"
  if [ -f "$ROOT_DIR/dist/index.d.ts" ]; then
    cp "$ROOT_DIR/dist/index.d.ts" \
      "$ROOT_DIR/entry/src/main/cpp/types/libhanyconnect_ui/Index.d.ts"
  fi
  if [ -f "$OHOS_NDK_HOME/native/llvm/lib/aarch64-linux-ohos/libc++_shared.so" ]; then
    cp "$OHOS_NDK_HOME/native/llvm/lib/aarch64-linux-ohos/libc++_shared.so" \
      "$ROOT_DIR/entry/libs/arm64-v8a/"
  fi
  HVIGORW="${HVIGORW:-/Applications/DevEco-Studio.app/Contents/tools/hvigor/bin/hvigorw}"
  if [ ! -x "$HVIGORW" ]; then
    HVIGORW="/Users/ranger/Downloads/command-line-tools/bin/hvigorw"
  fi
  (cd "$ROOT_DIR" && "$HVIGORW" default@PackageHap --mode module \
    -p module=entry@default -p buildMode=release --no-daemon)
fi

if [ ! -f "$HAP_PATH" ]; then
  echo "HAP missing: $HAP_PATH" >&2
  exit 1
fi

if [ "$FORCE_STOP_APP" -eq 1 ]; then
  HDC_CMD shell aa force-stop "$BUNDLE_NAME" >/dev/null 2>&1 || true
fi
HDC_CMD uninstall "$BUNDLE_NAME" >/dev/null 2>&1 || true
HDC_CMD install -r "$HAP_PATH"

# Default auto profile for plain smoke when no server given
if [ -z "$SERVER" ] && [ "$AUTO_CONNECT" -eq 0 ]; then
  SERVER="vpn.example.com"
  AUTO_CONNECT=1
  DRY_RUN=1
fi

# Harmony aa start uses --ps/--pb for Want parameters (not -p).
set --
set -- "$@" --ps hanyDryRun "$( [ "$DRY_RUN" -eq 1 ] && echo true || echo false )"
if [ -n "$SERVER" ]; then
  set -- "$@" --ps hanyServer "$SERVER"
fi
if [ -n "$NAME" ]; then
  set -- "$@" --ps hanyName "$NAME"
fi
if [ -n "$GROUP" ]; then
  set -- "$@" --ps hanyGroup "$GROUP"
fi
if [ -n "$USERNAME" ]; then
  set -- "$@" --ps hanyUsername "$USERNAME"
fi
if [ -n "$PASSWORD" ]; then
  set -- "$@" --ps hanyPassword "$PASSWORD"
fi
if [ "$ACCEPT_UNTRUSTED" -eq 1 ]; then
  set -- "$@" --pb hanyAcceptUntrusted true
fi
if [ "$AUTO_CONNECT" -eq 1 ]; then
  set -- "$@" --pb hanyAutoConnect true
fi
if [ "$EXPECT_CONNECTED" -eq 1 ]; then
  set -- "$@" --pb hanyExpectConnected true
fi
if [ "$EXPECT_FAILURE" -eq 1 ]; then
  set -- "$@" --pb hanyExpectFailure true
fi

echo "starting $BUNDLE_NAME/$ABILITY_NAME"
HDC_CMD shell aa start -a "$ABILITY_NAME" -b "$BUNDLE_NAME" "$@"

echo "capturing hilog for ${HILOG_SECONDS}s → $LOG_FILE"
# hilog -x dumps buffer; also sample live
{
  HDC_CMD shell "hilog -x" || true
  sleep "$HILOG_SECONDS"
  HDC_CMD shell "hilog -x" || true
} >"$LOG_FILE" 2>&1 || true

fail=0
require_marker() {
  pattern="$1"
  label="$2"
  if grep -E "$pattern" "$LOG_FILE" >/dev/null 2>&1; then
    echo "OK  $label"
  else
    echo "FAIL $label (pattern: $pattern)" >&2
    fail=1
  fi
}

require_marker "HAnyConnectEntry|HAnyConnectVpn|configured native home" "app launched"
require_marker "registered native platform callbacks|configured native home" "native shell ready"

if [ "$AUTO_CONNECT" -eq 1 ]; then
  require_marker "e2e config applied|e2e_config_applied|HAnyConnectE2E" "e2e config / markers"
  if [ "$EXPECT_FAILURE" -eq 1 ]; then
    require_marker "connect_auth_failed|e2e automation failed|VPN start failed|platform_vpn_failed" "expected failure observed"
  else
    # dry-run or successful platform orchestration
    if ! grep -E "connect_auth_ok|session_connected|e2e connect result|platform_vpn_running|backend_dry_run|backend_platform" "$LOG_FILE" >/dev/null 2>&1; then
      if [ "$ALLOW_VPN_UNSUPPORTED" -eq 1 ]; then
        echo "WARN connect markers missing but --allow-vpn-unsupported set"
      else
        echo "FAIL connect markers missing" >&2
        fail=1
      fi
    else
      echo "OK  connect orchestration markers"
    fi
  fi
fi

if [ "$EXPECT_CONNECTED" -eq 1 ]; then
  require_marker "session_connected|platform_vpn_running.:true|\"lifecycle\":\"connected\"" "connected lifecycle"
fi

if [ "$fail" -ne 0 ]; then
  echo "E2E failed. Log: $LOG_FILE" >&2
  tail -n 80 "$LOG_FILE" >&2 || true
  exit 1
fi

echo "device E2E OK"
echo "log: $LOG_FILE"
