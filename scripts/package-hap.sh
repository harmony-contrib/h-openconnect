#!/usr/bin/env sh
set -eu

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OHRS="${OHRS:-ohrs}"
ARCH="${ARCH:-aarch}"
NATIVE_PROFILE="${NATIVE_PROFILE:-release}"
HAP_BUILD_MODE="${HAP_BUILD_MODE:-release}"
# Full anyconnect-rs integration by default. Set FEATURES= to build UI-only shell.
FEATURES="${FEATURES:-native-anyconnect}"
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
  release|debug)
    ;;
  *)
    echo "Unsupported HAP_BUILD_MODE: $HAP_BUILD_MODE (expected release or debug)" >&2
    exit 1
    ;;
esac
SO_DST="$ROOT_DIR/entry/libs/arm64-v8a/libhopenconnect_ui.so"
HAP_PATH="${HAP_PATH:-$ROOT_DIR/entry/build/default/outputs/default/entry-default-unsigned.hap}"
SIGNED_HAP_PATH="${SIGNED_HAP_PATH:-$ROOT_DIR/entry/build/default/outputs/default/entry-default-signed.hap}"
SIGN_HAP="${SIGN_HAP:-0}"
HVIGOR_ARGS="${HVIGOR_ARGS:---no-daemon}"
DEVECO_STUDIO_HOME="${DEVECO_STUDIO_HOME:-/Applications/DevEco-Studio.app/Contents}"

case "$SIGN_HAP" in
  0|1)
    ;;
  *)
    echo "Unsupported SIGN_HAP: $SIGN_HAP (expected 0 or 1)" >&2
    exit 1
    ;;
esac

if [ -n "${HVIGORW:-}" ]; then
  HVIGORW_BIN="$HVIGORW"
elif [ -x "$ROOT_DIR/hvigorw" ]; then
  HVIGORW_BIN="$ROOT_DIR/hvigorw"
elif [ -x "$DEVECO_STUDIO_HOME/tools/hvigor/bin/hvigorw" ]; then
  HVIGORW_BIN="$DEVECO_STUDIO_HOME/tools/hvigor/bin/hvigorw"
elif [ -x "/Users/ranger/Downloads/command-line-tools/bin/hvigorw" ]; then
  HVIGORW_BIN="/Users/ranger/Downloads/command-line-tools/bin/hvigorw"
else
  HVIGORW_BIN="$(command -v hvigorw)"
fi

if [ -z "${DEVECO_SDK_HOME:-}" ] && [ -d "$DEVECO_STUDIO_HOME/sdk" ]; then
  export DEVECO_SDK_HOME="$DEVECO_STUDIO_HOME/sdk"
fi
if [ -z "${OHOS_NDK_HOME:-}" ] && [ -d "$DEVECO_STUDIO_HOME/sdk/default/openharmony" ]; then
  export OHOS_NDK_HOME="$DEVECO_STUDIO_HOME/sdk/default/openharmony"
fi
if [ -n "${DEVECO_JAVA_HOME:-}" ]; then
  export JAVA_HOME="$DEVECO_JAVA_HOME"
elif [ -d "$DEVECO_STUDIO_HOME/jbr/Contents/Home" ]; then
  export JAVA_HOME="$DEVECO_STUDIO_HOME/jbr/Contents/Home"
fi
if [ -z "${NODE_HOME:-}" ] && [ -x "$DEVECO_STUDIO_HOME/tools/node/bin/node" ]; then
  export NODE_HOME="$DEVECO_STUDIO_HOME/tools/node"
fi
if [ -n "${JAVA_HOME:-}" ]; then
  export PATH="$JAVA_HOME/bin:$PATH"
fi

cd "$ROOT_DIR"

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

# Bundle libc++_shared when the native library depends on it.
if [ -n "${OHOS_NDK_HOME:-}" ] && command -v llvm-readelf >/dev/null 2>&1 || [ -x "${OHOS_NDK_HOME:-}/native/llvm/bin/llvm-readelf" ]; then
  READELF="${OHOS_NDK_HOME}/native/llvm/bin/llvm-readelf"
  if [ -x "$READELF" ] && "$READELF" -d "$SO_SRC" | grep -q '\[libc++_shared\.so\]'; then
    CXX_SHARED="$OHOS_NDK_HOME/native/llvm/lib/aarch64-linux-ohos/libc++_shared.so"
    if [ -f "$CXX_SHARED" ]; then
      cp "$CXX_SHARED" "$ROOT_DIR/entry/libs/arm64-v8a/libc++_shared.so"
    fi
  fi
fi

# Sync generated d.ts into entry types (ohrs writes workspace dist/ by default).
if [ -f "$ROOT_DIR/dist/index.d.ts" ]; then
  cp "$ROOT_DIR/dist/index.d.ts" \
    "$ROOT_DIR/entry/src/main/cpp/types/libhopenconnect_ui/Index.d.ts"
elif [ -f "$ROOT_DIR/crates/hopenconnect_ui/dist/index.d.ts" ]; then
  cp "$ROOT_DIR/crates/hopenconnect_ui/dist/index.d.ts" \
    "$ROOT_DIR/entry/src/main/cpp/types/libhopenconnect_ui/Index.d.ts"
fi

if [ -x "${OHPM:-}" ]; then
  OHPM_BIN="$OHPM"
elif [ -x "$DEVECO_STUDIO_HOME/tools/ohpm/bin/ohpm" ]; then
  OHPM_BIN="$DEVECO_STUDIO_HOME/tools/ohpm/bin/ohpm"
elif [ -x "/Users/ranger/Downloads/command-line-tools/bin/ohpm" ]; then
  OHPM_BIN="/Users/ranger/Downloads/command-line-tools/bin/ohpm"
elif command -v ohpm >/dev/null 2>&1; then
  OHPM_BIN="$(command -v ohpm)"
else
  OHPM_BIN=""
fi
if [ -n "$OHPM_BIN" ]; then
  "$OHPM_BIN" install --all
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
