#!/usr/bin/env sh
# Source this before ohrs/cargo builds that link anyconnect-rs for OpenHarmony.
#
#   . ./scripts/env-ohos-anyconnect.sh
#   ohrs build --arch aarch --release -p hanyconnect_ui -- --features native-anyconnect
#
# Requires:
# - OHOS_NDK_HOME / OHOS_SDK_NATIVE (DevEco OpenHarmony Native SDK)
# - static libxml2 for aarch64-unknown-linux-ohos (auto-built if missing)
# - OpenSSL: prefer vendored-openssl feature (default with native-anyconnect).
#   Optional prebuilt: OPENSSL_PREFIX / ohos-openssl arm64-v8a.

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
ANYCONNECT_RS_DIR="${ANYCONNECT_RS_DIR:-$ROOT_DIR/../anyconnect-rs}"

export OHOS_NDK_HOME="${OHOS_NDK_HOME:-/Applications/DevEco-Studio.app/Contents/sdk/default/openharmony}"
export OHOS_SDK_NATIVE="${OHOS_SDK_NATIVE:-$OHOS_NDK_HOME/native}"
export DEVECO_SDK_HOME="${DEVECO_SDK_HOME:-/Applications/DevEco-Studio.app/Contents/sdk}"

if [ ! -d "$OHOS_SDK_NATIVE/sysroot" ] || [ ! -x "$OHOS_SDK_NATIVE/llvm/bin/clang" ]; then
  echo "OHOS_SDK_NATIVE is incomplete: $OHOS_SDK_NATIVE" >&2
  return 1 2>/dev/null || exit 1
fi

# Prefer anyconnect-rs official OHOS libxml2 prefix, then project third_party.
LIBXML2_PREFIX="${LIBXML2_PREFIX:-}"
if [ -z "$LIBXML2_PREFIX" ]; then
  if [ -f "$ANYCONNECT_RS_DIR/target/ohos-deps/aarch64-unknown-linux-ohos/lib/libxml2.a" ]; then
    LIBXML2_PREFIX="$ANYCONNECT_RS_DIR/target/ohos-deps/aarch64-unknown-linux-ohos"
  elif [ -f "$ROOT_DIR/third_party/libxml2-ohos-aarch64/lib/libxml2.a" ]; then
    LIBXML2_PREFIX="$ROOT_DIR/third_party/libxml2-ohos-aarch64"
  fi
fi

if [ -z "$LIBXML2_PREFIX" ] || [ ! -f "$LIBXML2_PREFIX/include/libxml2/libxml/parser.h" ]; then
  if [ -x "$ANYCONNECT_RS_DIR/tests/platform/ohos/build-libxml2.sh" ]; then
    echo "building libxml2 for aarch64-unknown-linux-ohos via anyconnect-rs…"
    LIBXML2_PREFIX="$("$ANYCONNECT_RS_DIR/tests/platform/ohos/build-libxml2.sh" aarch64-unknown-linux-ohos | tail -n 1)"
  elif [ -x "$ROOT_DIR/scripts/build-libxml2-ohos.sh" ]; then
    echo "building libxml2 via scripts/build-libxml2-ohos.sh…"
    "$ROOT_DIR/scripts/build-libxml2-ohos.sh"
    LIBXML2_PREFIX="$ROOT_DIR/third_party/libxml2-ohos-aarch64"
  else
    echo "libxml2 OHOS prefix missing and no build script found" >&2
    return 1 2>/dev/null || exit 1
  fi
fi

if [ ! -f "$LIBXML2_PREFIX/include/libxml2/libxml/parser.h" ]; then
  echo "libxml2 OHOS headers missing under $LIBXML2_PREFIX" >&2
  return 1 2>/dev/null || exit 1
fi

export AARCH64_UNKNOWN_LINUX_OHOS_ANYCONNECT_LIBXML2_DIR="$LIBXML2_PREFIX"
export ANYCONNECT_LIBXML2_DIR="$LIBXML2_PREFIX"

# Optional prebuilt OpenSSL (native-anyconnect defaults to vendored-openssl).
OPENSSL_PREFIX="${OPENSSL_PREFIX:-/Volumes/PSSD/code/ohos-rs/ohos-openssl/prelude/arm64-v8a}"
if [ -f "$OPENSSL_PREFIX/include/openssl/ssl.h" ] && [ "${OHOS_ANYCONNECT_VENDORED_OPENSSL:-1}" != "1" ]; then
  export AARCH64_UNKNOWN_LINUX_OHOS_ANYCONNECT_OPENSSL_DIR="$OPENSSL_PREFIX"
  export OPENSSL_DIR="$OPENSSL_PREFIX"
  export OPENSSL_LIB_DIR="$OPENSSL_PREFIX/lib"
  export OPENSSL_INCLUDE_DIR="$OPENSSL_PREFIX/include"
  export OPENSSL_STATIC=1
  export OPENSSL_NO_VENDOR=1
  OPENSSL_MODE="prebuilt:$OPENSSL_PREFIX"
else
  # Let openssl-sys / anyconnect vendored-openssl compile for OHOS.
  unset OPENSSL_DIR OPENSSL_LIB_DIR OPENSSL_INCLUDE_DIR OPENSSL_NO_VENDOR 2>/dev/null || true
  OPENSSL_MODE="vendored-openssl"
fi

# Toolchain hints for cargo/cc when ohrs does not inject them.
export CC_aarch64_unknown_linux_ohos="${CC_aarch64_unknown_linux_ohos:-$OHOS_SDK_NATIVE/llvm/bin/aarch64-unknown-linux-ohos-clang}"
export AR_aarch64_unknown_linux_ohos="${AR_aarch64_unknown_linux_ohos:-$OHOS_SDK_NATIVE/llvm/bin/llvm-ar}"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_OHOS_LINKER="${CARGO_TARGET_AARCH64_UNKNOWN_LINUX_OHOS_LINKER:-$OHOS_SDK_NATIVE/llvm/bin/aarch64-unknown-linux-ohos-clang}"

# Real session by default in the packaged app (override with HANYCONNECT_DRY_RUN=1).
export HANYCONNECT_DRY_RUN="${HANYCONNECT_DRY_RUN:-0}"

echo "OHOS anyconnect env ready"
echo "  OHOS_SDK_NATIVE=$OHOS_SDK_NATIVE"
echo "  ANYCONNECT_LIBXML2_DIR=$LIBXML2_PREFIX"
echo "  OPENSSL=$OPENSSL_MODE"
echo "  HANYCONNECT_DRY_RUN=$HANYCONNECT_DRY_RUN"
