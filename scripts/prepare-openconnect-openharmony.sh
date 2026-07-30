#!/usr/bin/env sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PATCH_FILE="$ROOT_DIR/patches/openconnect/0001-allow-openharmony-reported-os.patch"
ANYCONNECT_SYS_VERSION="0.1.0"
OPENCONNECT_VERSION="9.20"

for command_name in cargo jq patch; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "required command is unavailable: $command_name" >&2
    exit 1
  fi
done

if [ ! -f "$PATCH_FILE" ]; then
  echo "OpenHarmony OpenConnect patch is missing: $PATCH_FILE" >&2
  exit 1
fi

if command -v shasum >/dev/null 2>&1; then
  PATCH_HASH="$(shasum -a 256 "$PATCH_FILE" | awk '{print $1}')"
elif command -v sha256sum >/dev/null 2>&1; then
  PATCH_HASH="$(sha256sum "$PATCH_FILE" | awk '{print $1}')"
else
  echo "required SHA-256 command is unavailable" >&2
  exit 1
fi

METADATA="$(
  cd "$ROOT_DIR" &&
    cargo metadata --format-version 1 --locked \
      --filter-platform aarch64-unknown-linux-ohos \
      --features hanyconnect_ui/native-anyconnect
)"
ANYCONNECT_MANIFEST="$(
  printf '%s\n' "$METADATA" |
    jq -r --arg version "$ANYCONNECT_SYS_VERSION" \
      '[.packages[] | select(.name == "anyconnect-sys" and .version == $version)][0].manifest_path // ""'
)"
if [ -z "$ANYCONNECT_MANIFEST" ] || [ ! -f "$ANYCONNECT_MANIFEST" ]; then
  echo "anyconnect-sys $ANYCONNECT_SYS_VERSION is not available in Cargo metadata" >&2
  exit 1
fi

SOURCE_DIR="$(dirname -- "$ANYCONNECT_MANIFEST")/vendor/openconnect"
if [ ! -f "$SOURCE_DIR/openconnect.h" ] || [ ! -f "$SOURCE_DIR/library.c" ]; then
  echo "OpenConnect source is incomplete under $SOURCE_DIR" >&2
  exit 1
fi
if ! grep -Fq "AC_INIT([openconnect], [$OPENCONNECT_VERSION])" "$SOURCE_DIR/configure.ac"; then
  echo "expected OpenConnect $OPENCONNECT_VERSION source under $SOURCE_DIR" >&2
  exit 1
fi
if ! grep -Fq \
  'static const char * const allowed[] = {"linux", "linux-64", "win", "mac-intel", "android", "apple-ios"};' \
  "$SOURCE_DIR/library.c"; then
  echo "OpenConnect reported-OS implementation differs from the pinned source" >&2
  exit 1
fi

OUTPUT_DIR="$ROOT_DIR/target/openconnect-openharmony-$PATCH_HASH"
STAMP_FILE="$OUTPUT_DIR/.hanyconnect-openharmony-patch"
EXPECTED_STAMP="anyconnect-sys=$ANYCONNECT_SYS_VERSION openconnect=$OPENCONNECT_VERSION patch=$PATCH_HASH"

if [ -f "$STAMP_FILE" ] &&
  [ "$(sed -n '1p' "$STAMP_FILE")" = "$EXPECTED_STAMP" ] &&
  grep -Fq '"android", "apple-ios", "OpenHarmony"};' "$OUTPUT_DIR/library.c"; then
  printf '%s\n' "$OUTPUT_DIR"
  exit 0
fi

case "$OUTPUT_DIR" in
  "$ROOT_DIR"/target/openconnect-openharmony-*) ;;
  *)
    echo "refusing unexpected OpenConnect output path: $OUTPUT_DIR" >&2
    exit 1
    ;;
esac

mkdir -p "$ROOT_DIR/target"
TEMP_DIR="$(mktemp -d "$ROOT_DIR/target/.openconnect-openharmony.XXXXXX")"
cleanup() {
  if [ -n "${TEMP_DIR:-}" ] && [ -d "$TEMP_DIR" ]; then
    rm -rf -- "$TEMP_DIR"
  fi
}
trap cleanup EXIT HUP INT TERM

cp -R "$SOURCE_DIR/." "$TEMP_DIR/"
patch -d "$TEMP_DIR" -p1 < "$PATCH_FILE" >/dev/null
printf '%s\n' "$EXPECTED_STAMP" > "$TEMP_DIR/.hanyconnect-openharmony-patch"

if [ -e "$OUTPUT_DIR" ]; then
  rm -rf -- "$OUTPUT_DIR"
fi
mv "$TEMP_DIR" "$OUTPUT_DIR"
TEMP_DIR=""

printf '%s\n' "$OUTPUT_DIR"
