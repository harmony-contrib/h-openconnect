#!/usr/bin/env sh
# Cross-compile a minimal static libxml2 for aarch64-linux-ohos.
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
OHOS_NDK_HOME="${OHOS_NDK_HOME:-/Applications/DevEco-Studio.app/Contents/sdk/default/openharmony}"
OHOS_SDK_NATIVE="${OHOS_SDK_NATIVE:-$OHOS_NDK_HOME/native}"
PREFIX="${PREFIX:-$ROOT_DIR/third_party/libxml2-ohos-aarch64}"
VERSION="${LIBXML2_VERSION:-2.12.9}"
SRC_ROOT="${SRC_ROOT:-/tmp/libxml2-ohos-src}"
TARBALL="/tmp/libxml2-${VERSION}.tar.xz"
URL="https://download.gnome.org/sources/libxml2/2.12/libxml2-${VERSION}.tar.xz"

CLANG="$OHOS_SDK_NATIVE/llvm/bin/aarch64-unknown-linux-ohos-clang"
SYSROOT="$OHOS_SDK_NATIVE/sysroot"
test -x "$CLANG"
test -d "$SYSROOT"

if [ ! -f "$TARBALL" ]; then
  curl -fsSL "$URL" -o "$TARBALL"
fi

rm -rf "$SRC_ROOT" "$PREFIX"
mkdir -p "$SRC_ROOT"
tar -xJf "$TARBALL" -C "$SRC_ROOT"
cd "$SRC_ROOT/libxml2-${VERSION}"

export CC="$CLANG"
export AR="$OHOS_SDK_NATIVE/llvm/bin/llvm-ar"
export RANLIB="$OHOS_SDK_NATIVE/llvm/bin/llvm-ranlib"
export CFLAGS="--target=aarch64-linux-ohos --sysroot=$SYSROOT -fPIC -O2"
export LDFLAGS="--target=aarch64-linux-ohos --sysroot=$SYSROOT"

./configure \
  --host=aarch64-unknown-linux-gnu \
  --build="$(uname -m)-apple-darwin" \
  --prefix="$PREFIX" \
  --without-python \
  --without-lzma \
  --without-zlib \
  --without-iconv \
  --without-icu \
  --without-http \
  --without-ftp \
  --without-legacy \
  --without-modules \
  --disable-shared \
  --enable-static \
  ac_cv_func_malloc_0_nonnull=yes \
  ac_cv_func_realloc_0_nonnull=yes

# config.status can claim config.h is "unchanged" without creating it.
./config.status --file=config.h >/dev/null 2>&1 || true
test -f config.h || ./config.status config.h

make -j"$(sysctl -n hw.ncpu 2>/dev/null || echo 4)"
make install

test -f "$PREFIX/lib/libxml2.a"
test -f "$PREFIX/include/libxml2/libxml/parser.h"
echo "Installed libxml2 → $PREFIX"
