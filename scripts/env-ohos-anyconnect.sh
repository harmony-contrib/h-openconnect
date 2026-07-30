#!/usr/bin/env sh
# Source this before ohrs/cargo builds that link anyconnect-rs for OpenHarmony.
#
#   . ./scripts/env-ohos-anyconnect.sh
#   ohrs build --arch aarch --release -p hopenconnect_ui -- --features native-anyconnect
#
# Requires:
# - OHOS_NDK_HOME / OHOS_SDK_NATIVE (DevEco OpenHarmony Native SDK)
# - libxml2 is built from anyconnect-sys's bundled source.
# - OpenSSL: prefer vendored-openssl feature (default with native-anyconnect).
#   Optional prebuilt: OPENSSL_PREFIX / ohos-openssl arm64-v8a.

export OHOS_NDK_HOME="${OHOS_NDK_HOME:-/Applications/DevEco-Studio.app/Contents/sdk/default/openharmony}"
export OHOS_SDK_NATIVE="${OHOS_SDK_NATIVE:-$OHOS_NDK_HOME/native}"
export DEVECO_SDK_HOME="${DEVECO_SDK_HOME:-/Applications/DevEco-Studio.app/Contents/sdk}"

if [ ! -d "$OHOS_SDK_NATIVE/sysroot" ] || [ ! -x "$OHOS_SDK_NATIVE/llvm/bin/clang" ]; then
  echo "OHOS_SDK_NATIVE is incomplete: $OHOS_SDK_NATIVE" >&2
  return 1 2>/dev/null || exit 1
fi

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

# Real session by default in the packaged app (override with HOPENCONNECT_DRY_RUN=1).
export HOPENCONNECT_DRY_RUN="${HOPENCONNECT_DRY_RUN:-0}"

echo "OHOS anyconnect env ready"
echo "  OHOS_SDK_NATIVE=$OHOS_SDK_NATIVE"
echo "  LIBXML2=vendored-libxml2"
echo "  OPENSSL=$OPENSSL_MODE"
echo "  HOPENCONNECT_DRY_RUN=$HOPENCONNECT_DRY_RUN"
