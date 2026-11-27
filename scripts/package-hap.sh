#!/usr/bin/env sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
. "$ROOT_DIR/scripts/ohos-env.sh"

OHRS="${OHRS:-ohrs}"
ARCH="${ARCH:-aarch}"
NATIVE_PROFILE="${NATIVE_PROFILE:-release}"
HAP_BUILD_MODE="${HAP_BUILD_MODE:-release}"
# Full anyconnect-rs integration by default. Set FEATURES= to build UI-only shell.
FEATURES="${FEATURES:-native-anyconnect}"
SIGN_HAP="${SIGN_HAP:-0}"
PACKAGE_APP="${PACKAGE_APP:-0}"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --signed)
      SIGN_HAP=1
      shift
      ;;
    --app)
      PACKAGE_APP=1
      shift
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 2
      ;;
  esac
done

case "$NATIVE_PROFILE" in
  release)
    SO_SRC="$ROOT_DIR/target/aarch64-unknown-linux-ohos/release/libhopenconnect_ui.so"
    ;;
  debug)
    SO_SRC="$ROOT_DIR/target/aarch64-unknown-linux-ohos/debug/libhopenconnect_ui.so"
    ;;
  *)
    echo "Unsupported NATIVE_PROFILE: $NATIVE_PROFILE (expected release or debug)" >&2
    exit 1
    ;;
esac

case "$HAP_BUILD_MODE" in
  release|debug) ;;
  *)
    echo "Unsupported HAP_BUILD_MODE: $HAP_BUILD_MODE (expected release or debug)" >&2
    exit 1
    ;;
esac

case "$SIGN_HAP" in
  0|1) ;;
  *)
    echo "Unsupported SIGN_HAP: $SIGN_HAP (expected 0 or 1)" >&2
    exit 1
    ;;
esac

SO_DST="$ROOT_DIR/entry/libs/arm64-v8a/libhopenconnect_ui.so"
HAP_PATH="${HAP_PATH:-$ROOT_DIR/entry/build/default/outputs/default/entry-default-unsigned.hap}"
SIGNED_HAP_PATH="${SIGNED_HAP_PATH:-$ROOT_DIR/entry/build/default/outputs/default/entry-default-signed.hap}"
HVIGOR_ARGS="${HVIGOR_ARGS:---no-daemon}"

if [ -n "${HVIGORW:-}" ]; then
  HVIGORW_BIN="$HVIGORW"
elif [ -x "$ROOT_DIR/hvigorw" ]; then
  HVIGORW_BIN="$ROOT_DIR/hvigorw"
elif [ -x "${DEVECO_STUDIO_HOME:-}/tools/hvigor/bin/hvigorw" ]; then
  HVIGORW_BIN="$DEVECO_STUDIO_HOME/tools/hvigor/bin/hvigorw"
elif command -v hvigorw >/dev/null 2>&1; then
  HVIGORW_BIN="$(command -v hvigorw)"
else
  echo "Hvigor was not found. Set HVIGORW or DEVECO_STUDIO_HOME." >&2
  exit 127
fi

cd "$ROOT_DIR"

# 1. Build NSH HarmonyOS runtime, stage @nsh/harmonyos ArkTS, and bundle ui/nsh/.
NATIVE_PROFILE="$NATIVE_PROFILE" "$ROOT_DIR/scripts/build-nsh-ohos.sh"

# 2. Build H-OpenConnect N-API native library (hopenconnect_ui -> hopenconnect_app + hopenconnect_core).
CARGO_FEATURE_ARGS=""
if [ -n "$FEATURES" ]; then
  # shellcheck disable=SC1091
  . "$ROOT_DIR/scripts/env-ohos-anyconnect.sh"
  CARGO_FEATURE_ARGS="--features $FEATURES"
fi

if [ "$NATIVE_PROFILE" = "release" ]; then
  # shellcheck disable=SC2086
  "$OHRS" build --arch "$ARCH" --release -p hopenconnect_ui -- $CARGO_FEATURE_ARGS
else
  # shellcheck disable=SC2086
  "$OHRS" build --arch "$ARCH" -p hopenconnect_ui -- $CARGO_FEATURE_ARGS
fi

mkdir -p "$(dirname "$SO_DST")"
cp "$SO_SRC" "$SO_DST"

# Bundle libc++_shared for arm64-v8a native libraries (required by NSH and/or anyconnect).
for cxx_candidate in \
  "${OHOS_NDK_HOME:-}/native/llvm/lib/aarch64-linux-ohos/libc++_shared.so" \
  "${OHOS_NDK_HOME:-}/native/llvm/lib/aarch64-linux-ohos/c++/libc++_shared.so"
do
  if [ -f "$cxx_candidate" ]; then
    cp "$cxx_candidate" "$ROOT_DIR/entry/libs/arm64-v8a/libc++_shared.so"
    break
  fi
done

# Sync generated d.ts into entry types.
if [ -f "$ROOT_DIR/dist/index.d.ts" ]; then
  cp "$ROOT_DIR/dist/index.d.ts" \
    "$ROOT_DIR/entry/src/main/cpp/types/libhopenconnect_ui/Index.d.ts"
elif [ -f "$ROOT_DIR/crates/hopenconnect_ui/dist/index.d.ts" ]; then
  cp "$ROOT_DIR/crates/hopenconnect_ui/dist/index.d.ts" \
    "$ROOT_DIR/entry/src/main/cpp/types/libhopenconnect_ui/Index.d.ts"
fi

if command -v ohpm >/dev/null 2>&1; then
  ohpm install --all
fi

# 3. Package HarmonyOS APP or HAP via Hvigor.
if [ "$PACKAGE_APP" = "1" ]; then
  APP_PATH="${APP_PATH:-$ROOT_DIR/build/outputs/default/h-anyconnect-default-unsigned.app}"
  SIGNED_APP_PATH="${SIGNED_APP_PATH:-$ROOT_DIR/build/outputs/default/h-anyconnect-default-signed.app}"
  # shellcheck disable=SC2086
  "$HVIGORW_BIN" assembleApp --mode project -p product=default \
    -p buildMode="$HAP_BUILD_MODE" $HVIGOR_ARGS

  if [ ! -f "$APP_PATH" ]; then
    echo "Expected unsigned APP was not generated: $APP_PATH" >&2
    exit 1
  fi

  if [ "$SIGN_HAP" = "1" ]; then
    if [ ! -f "$SIGNED_APP_PATH" ]; then
      echo "Expected signed APP was not generated: $SIGNED_APP_PATH" >&2
      exit 1
    fi
    if [ -z "$(find "$SIGNED_APP_PATH" -newer "$APP_PATH" -print)" ]; then
      echo "Signed APP is stale relative to the unsigned build: $SIGNED_APP_PATH" >&2
      exit 1
    fi
    echo "$SIGNED_APP_PATH"
  else
    echo "$APP_PATH"
  fi
  exit 0
fi

SIGN_ARGS=""
HVIGOR_TASK="default@PackageHap"
if [ "$SIGN_HAP" = "1" ]; then
  HVIGOR_TASK="default@SignHap"
  SIGN_ARGS="-p enableSignTask=true"
fi

# shellcheck disable=SC2086
"$HVIGORW_BIN" "$HVIGOR_TASK" --mode module -p module=entry@default \
  -p buildMode="$HAP_BUILD_MODE" $HVIGOR_ARGS $SIGN_ARGS

if [ ! -f "$HAP_PATH" ]; then
  echo "Expected unsigned HAP was not generated: $HAP_PATH" >&2
  exit 1
fi

if [ "$SIGN_HAP" = "1" ]; then
  if [ ! -f "$SIGNED_HAP_PATH" ]; then
    echo "Expected signed HAP was not generated: $SIGNED_HAP_PATH" >&2
    exit 1
  fi
  if [ -z "$(find "$SIGNED_HAP_PATH" -newer "$HAP_PATH" -print)" ]; then
    echo "Signed HAP is stale relative to the unsigned build: $SIGNED_HAP_PATH" >&2
    exit 1
  fi
  echo "$SIGNED_HAP_PATH"
else
  echo "$HAP_PATH"
fi
