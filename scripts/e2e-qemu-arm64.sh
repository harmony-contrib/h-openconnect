#!/usr/bin/env bash
# Full production-path VPN E2E against an ARM64 OpenHarmony QEMU image.
#
# The application receives no test-only Want parameters. The test installs a
# normal signed HAP, writes a normal connection profile into the app sandbox,
# and drives the fixed 800x500 QEMU UI with the platform uitest tool.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
QEMU_PACKAGE_DIR="${QEMU_PACKAGE_DIR:-}"
HAP_PATH="${HAP_PATH:-$ROOT_DIR/entry/build/default/outputs/default/entry-default-signed.hap}"
HDC="${HDC:-hdc}"
HDC_HOST_PORT="${QEMU_HDC_HOST_PORT:-5558}"
HDC_TARGET="${HDC_TARGET:-127.0.0.1:${HDC_HOST_PORT}}"
QEMU_ACCEL="${QEMU_ACCEL:-auto}"
QEMU_BOOT_TIMEOUT="${QEMU_BOOT_TIMEOUT:-240}"
VPN_START_TIMEOUT="${VPN_START_TIMEOUT:-100}"
VPN_DEADLINE_TIMEOUT="${VPN_DEADLINE_TIMEOUT:-150}"
RUN_DEADLINE_TEST="${RUN_DEADLINE_TEST:-1}"
BUNDLE_NAME="${BUNDLE_NAME:-com.richerfu.h_openconnect}"
ABILITY_NAME="${ABILITY_NAME:-EntryAbility}"
OCSERV_PORT="${OCSERV_PORT:-14433}"
OCSERV_USER="${OCSERV_USER:-demo}"
OCSERV_PASS="${OCSERV_PASS:-demo}"
RUN_ID="${GITHUB_RUN_ID:-local}-$(date +%s)-$$"
OCSERV_NAME="${OCSERV_NAME:-hopenconnect-ocserv-ci-${RUN_ID}}"
ARTIFACT_DIR="${ARTIFACT_DIR:-$ROOT_DIR/smoke-logs/qemu-ci-${RUN_ID}}"
KEEP_QEMU_RUN_DIR="${KEEP_QEMU_RUN_DIR:-0}"

if [ -z "$QEMU_PACKAGE_DIR" ]; then
  echo "QEMU_PACKAGE_DIR is required" >&2
  exit 2
fi
if [ ! -f "$HAP_PATH" ]; then
  echo "signed HAP not found: $HAP_PATH" >&2
  exit 2
fi
if [ "$(uname -s)" != "Darwin" ] || [ "$(uname -m)" != "arm64" ]; then
  echo "this E2E requires an ARM64 macOS host (got $(uname -s)/$(uname -m))" >&2
  exit 2
fi

for command_name in qemu-system-aarch64 "$HDC" docker pgrep python3; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "required command not found: $command_name" >&2
    exit 2
  fi
done

QEMU_LAUNCHER="$QEMU_PACKAGE_DIR/launch/macos.command"
QEMU_MANIFEST="$QEMU_PACKAGE_DIR/manifest.json"
if [ ! -x "$QEMU_LAUNCHER" ] || [ ! -f "$QEMU_MANIFEST" ]; then
  echo "invalid ARM64 QEMU package: $QEMU_PACKAGE_DIR" >&2
  exit 2
fi
if ! grep -Eq '"guest_arch"[[:space:]]*:[[:space:]]*"arm64"' "$QEMU_MANIFEST" ||
   ! grep -Eq '"standard_vpn"[[:space:]]*:[[:space:]]*true' "$QEMU_MANIFEST"; then
  echo "QEMU package does not declare ARM64 standard VPN support" >&2
  exit 2
fi

mkdir -p "$ARTIFACT_DIR"
TEST_TMP="$(mktemp -d "${TMPDIR:-/tmp}/hopenconnect-qemu-ci.XXXXXX")"
QEMU_RUN_ROOT="$TEST_TMP/qemu"
QEMU_RUN_DIR="$QEMU_RUN_ROOT/$(basename "$QEMU_PACKAGE_DIR")"
OCSERV_DATA_DIR="$TEST_TMP/ocserv"
PROFILE_DIR="$TEST_TMP/profile"
QEMU_LOG="$ARTIFACT_DIR/qemu.log"
HILOG_FILE="$ARTIFACT_DIR/hilog.log"
ENTRY_HILOG_FILE="$ARTIFACT_DIR/hilog-entry.log"
LAYOUT_FILE="$ARTIFACT_DIR/layout.json"
GUEST_LAYOUT="/data/local/tmp/hopenconnect-ci-layout.json"
QEMU_PID=""
QEMU_CHILD_PID=""
HDC_READY=0
OCSERV_STARTED=0

hdc_cmd() {
  "$HDC" -t "$HDC_TARGET" "$@"
}

dump_layout() {
  hdc_cmd shell "uitest dumpLayout -p $GUEST_LAYOUT >/dev/null" >/dev/null 2>&1 || return 1
  hdc_cmd shell "cat $GUEST_LAYOUT" 2>/dev/null | tr -d '\r' >"$LAYOUT_FILE"
}

wait_layout_text() {
  local pattern="$1"
  local timeout="$2"
  local deadline=$((SECONDS + timeout))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if dump_layout && grep -Eq "$pattern" "$LAYOUT_FILE"; then
      return 0
    fi
    sleep 1
  done
  echo "timed out waiting for UI text: $pattern" >&2
  return 1
}

click() {
  hdc_cmd shell "uitest uiInput click $1 $2" >/dev/null
}

tun_exists() {
  hdc_cmd shell "ifconfig vpn-tun" 2>/dev/null | tr -d '\r' | \
    grep -Eq '^vpn-tun[[:space:]]'
}

wait_tun_absent() {
  local timeout="$1"
  local deadline=$((SECONDS + timeout))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if ! tun_exists; then
      return 0
    fi
    sleep 1
  done
  echo "vpn-tun still exists after ${timeout}s" >&2
  return 1
}

capture_hilog() {
  local output="$1"
  local filter="${2:-}"
  local collector_pid
  if [ -n "$filter" ]; then
    hdc_cmd shell "hilog -x | grep '$filter'" >"$output" 2>&1 &
  else
    hdc_cmd shell "hilog -x" >"$output" 2>&1 &
  fi
  collector_pid=$!
  for _ in $(seq 1 30); do
    if ! kill -0 "$collector_pid" >/dev/null 2>&1; then
      wait "$collector_pid" || true
      return 0
    fi
    sleep 0.5
  done
  kill -TERM "$collector_pid" >/dev/null 2>&1 || true
  wait "$collector_pid" >/dev/null 2>&1 || true
  echo "hilog capture reached its 15s diagnostic limit" >>"$output"
}

collect_artifacts() {
  if [ "$HDC_READY" = "1" ]; then
    capture_hilog "$HILOG_FILE" || true
    dump_layout || true
    hdc_cmd shell "ifconfig vpn-tun; netstat -rn" >"$ARTIFACT_DIR/network.txt" 2>&1 || true
  fi
  if [ "$OCSERV_STARTED" = "1" ]; then
    docker logs "$OCSERV_NAME" >"$ARTIFACT_DIR/ocserv.log" 2>&1 || true
    docker exec "$OCSERV_NAME" occtl show users >"$ARTIFACT_DIR/ocserv-users.txt" 2>&1 || true
  fi
}

resolve_qemu_child() {
  if [ -n "$QEMU_PID" ]; then
    pgrep -P "$QEMU_PID" -f 'qemu-system-aarch64' 2>/dev/null | head -n 1 || true
  fi
}

stop_qemu() {
  if [ -z "$QEMU_CHILD_PID" ]; then
    QEMU_CHILD_PID="$(resolve_qemu_child)"
  fi
  if [ -n "$QEMU_CHILD_PID" ] && kill -0 "$QEMU_CHILD_PID" >/dev/null 2>&1; then
    kill -TERM "$QEMU_CHILD_PID" >/dev/null 2>&1 || true
  fi
  if [ -n "$QEMU_PID" ] && kill -0 "$QEMU_PID" >/dev/null 2>&1; then
    for _ in $(seq 1 20); do
      kill -0 "$QEMU_PID" >/dev/null 2>&1 || break
      sleep 0.5
    done
    if kill -0 "$QEMU_PID" >/dev/null 2>&1; then
      kill -TERM "$QEMU_PID" >/dev/null 2>&1 || true
    fi
    wait "$QEMU_PID" >/dev/null 2>&1 || true
  fi
  if [ -n "$QEMU_CHILD_PID" ] && kill -0 "$QEMU_CHILD_PID" >/dev/null 2>&1; then
    kill -KILL "$QEMU_CHILD_PID" >/dev/null 2>&1 || true
  fi
}

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  set +e
  collect_artifacts
  if [ "$OCSERV_STARTED" = "1" ]; then
    OCSERV_NAME="$OCSERV_NAME" OCSERV_DATA_DIR="$OCSERV_DATA_DIR" \
      OCSERV_PORT="$OCSERV_PORT" "$ROOT_DIR/scripts/dev-ocserv.sh" stop >/dev/null 2>&1
  fi
  stop_qemu
  if [ "$KEEP_QEMU_RUN_DIR" != "1" ]; then
    rm -rf "$TEST_TMP"
  else
    echo "kept QEMU run directory: $TEST_TMP"
  fi
  exit "$status"
}
trap cleanup EXIT INT TERM

echo "==> clone clean ARM64 QEMU package"
mkdir -p "$QEMU_RUN_ROOT"
if ! cp -cR "$QEMU_PACKAGE_DIR" "$QEMU_RUN_DIR" 2>/dev/null; then
  rm -rf "$QEMU_RUN_DIR"
  cp -R "$QEMU_PACKAGE_DIR" "$QEMU_RUN_DIR"
fi

echo "==> start local AnyConnect headend"
OCSERV_NAME="$OCSERV_NAME" OCSERV_DATA_DIR="$OCSERV_DATA_DIR" \
  OCSERV_PORT="$OCSERV_PORT" OCSERV_USER="$OCSERV_USER" OCSERV_PASS="$OCSERV_PASS" \
  OCSERV_HOST_IP="10.0.2.2" "$ROOT_DIR/scripts/dev-ocserv.sh" start
OCSERV_STARTED=1

echo "==> boot ARM64 OpenHarmony QEMU"
QEMU_DISPLAY=none QEMU_ACCEL="$QEMU_ACCEL" QEMU_HDC_HOST_PORT="$HDC_HOST_PORT" \
  "$QEMU_RUN_DIR/launch/macos.command" >"$QEMU_LOG" 2>&1 &
QEMU_PID=$!

boot_deadline=$((SECONDS + QEMU_BOOT_TIMEOUT))
boot_ready_since=0
while [ "$SECONDS" -lt "$boot_deadline" ]; do
  if ! kill -0 "$QEMU_PID" >/dev/null 2>&1; then
    echo "QEMU exited during boot" >&2
    tail -n 120 "$QEMU_LOG" >&2 || true
    exit 1
  fi
  if [ -z "$QEMU_CHILD_PID" ]; then
    QEMU_CHILD_PID="$(resolve_qemu_child)"
  fi
  "$HDC" tconn "$HDC_TARGET" >/dev/null 2>&1 || true
  if hdc_cmd shell "param get bootevent.boot.completed" 2>/dev/null | grep -q true &&
     hdc_cmd shell "uname -m" 2>/dev/null | grep -q aarch64; then
    if [ "$boot_ready_since" -eq 0 ]; then
      boot_ready_since=$SECONDS
    elif [ $((SECONDS - boot_ready_since)) -ge 8 ]; then
      HDC_READY=1
      break
    fi
  else
    boot_ready_since=0
  fi
  sleep 2
done
if [ "$HDC_READY" != "1" ]; then
  echo "QEMU did not finish booting within ${QEMU_BOOT_TIMEOUT}s" >&2
  exit 1
fi

echo "==> install signed production HAP"
hdc_cmd shell "bm uninstall -n $BUNDLE_NAME" >/dev/null 2>&1 || true
install_output="$(hdc_cmd install "$HAP_PATH" 2>&1)"
printf '%s\n' "$install_output" | tee "$ARTIFACT_DIR/install.log"
if ! printf '%s\n' "$install_output" | grep -q "install bundle successfully"; then
  echo "HAP installation failed" >&2
  exit 1
fi

app_uid="$(hdc_cmd shell "bm dump -n $BUNDLE_NAME" | tr -d '\r' | \
  sed -n 's/.*"uid"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p' | head -n 1)"
if ! [[ "$app_uid" =~ ^[0-9]+$ ]]; then
  echo "failed to resolve application UID" >&2
  exit 1
fi
echo "application UID: $app_uid"

echo "==> provision a normal local ocserv profile"
mkdir -p "$PROFILE_DIR"
cat >"$PROFILE_DIR/connections.json" <<JSON
[
  {
    "id": "qemu-ci", "name": "QEMU CI ocserv", "server": "10.0.2.2:${OCSERV_PORT}",
    "group": "", "username": "${OCSERV_USER}", "password": "${OCSERV_PASS}",
    "protocol": "anyConnect", "authMethod": "password", "certificate": "",
    "privateKey": "", "secondaryCertificate": "", "secondaryPrivateKey": "",
    "caCertificate": "", "keyPassword": "", "secondaryKeyPassword": "",
    "httpProxy": "", "serverCertHash": "", "backupServers": "",
    "strictCertificateTrust": false, "blockUntrustedServers": false,
    "allowLocalLan": false, "forceGlobal": false, "splitTunnelMode": "auto",
    "splitTunnelNetworks": "", "connectOnDemand": false,
    "externalBrowserAuth": false, "fipsMode": false, "allowInsecureCrypto": false,
    "useDtls": false, "reportedOs": "OpenHarmony", "userAgent": "",
    "clientVersion": "", "sni": "", "requirePfs": false,
    "disableXmlPost": false, "dpdSeconds": 0, "softwareToken": "disabled",
    "tokenString": "", "csdWrapper": "", "trustedApplications": "",
    "blockedApplications": "", "mtu": 0, "favorite": true
  }
]
JSON
cat >"$PROFILE_DIR/preferences.json" <<'JSON'
{"activeConnectionId":"qemu-ci","language":"system","theme":"system"}
JSON

app_home="/data/app/el2/100/base/$BUNDLE_NAME/haps/entry/files/h-openconnect"
hdc_cmd shell "mkdir -p $app_home"
hdc_cmd file send "$PROFILE_DIR/connections.json" "$app_home/connections.json" >/dev/null
hdc_cmd file send "$PROFILE_DIR/preferences.json" "$app_home/preferences.json" >/dev/null
hdc_cmd shell "chown -R $app_uid:$app_uid $app_home; chmod 700 $app_home; chmod 600 $app_home/*.json"

hdc_cmd shell "hilog -G 8M >/dev/null; hilog -r >/dev/null; aa start -a $ABILITY_NAME -b $BUNDLE_NAME" >/dev/null
wait_layout_text 'QEMU CI ocserv' 30

if [ "$RUN_DEADLINE_TEST" = "1" ]; then
  echo "==> verify a pending API 24 authorization reaches the global deadline"
  click 400 340
  wait_layout_text '是否允许使用 VPN|Allow.*VPN|VPN.*Allow' 30
  click 306 316
  sleep 7
  dump_layout
  if grep -Eq '连接失败|Connection failed' "$LAYOUT_FILE"; then
    echo "VPN start failed before the global transaction deadline" >&2
    exit 1
  fi
  wait_layout_text '连接失败|Connection failed' "$VPN_DEADLINE_TIMEOUT"
  if tun_exists; then
    echo "vpn-tun exists after authorization cancellation" >&2
    exit 1
  fi
fi

echo "==> authorize and establish the real AnyConnect tunnel"
click 400 340
wait_layout_text '是否允许使用 VPN|Allow.*VPN|VPN.*Allow' 30
click 495 316
wait_layout_text '已连接|Connected' "$VPN_START_TIMEOUT"

hdc_cmd shell "ifconfig vpn-tun" | tee "$ARTIFACT_DIR/tun-connected.txt"
hdc_cmd shell "netstat -rn" | tee "$ARTIFACT_DIR/routes-connected.txt"
grep -Eq 'UP.*RUNNING|RUNNING.*UP' "$ARTIFACT_DIR/tun-connected.txt"
grep -q 'vpn-tun' "$ARTIFACT_DIR/routes-connected.txt"

docker exec "$OCSERV_NAME" occtl show users | tee "$ARTIFACT_DIR/ocserv-users-connected.txt"
grep -Eq "[[:space:]]${OCSERV_USER}[[:space:]].*connected" \
  "$ARTIFACT_DIR/ocserv-users-connected.txt"

echo "==> verify application-UID DNS and TCP through VPN policy routing"
OHOS_CLANG="${OHOS_CLANG:-${OHOS_NDK_HOME:-}/native/llvm/bin/aarch64-unknown-linux-ohos-clang}"
if [ ! -x "$OHOS_CLANG" ]; then
  echo "OpenHarmony aarch64 clang not found: $OHOS_CLANG" >&2
  exit 2
fi
"$OHOS_CLANG" "$ROOT_DIR/scripts/device-net-probe.c" -o "$TEST_TMP/device-net-probe"
hdc_cmd file send "$TEST_TMP/device-net-probe" /data/local/tmp/device-net-probe >/dev/null
hdc_cmd shell "chmod 755 /data/local/tmp/device-net-probe"
hdc_cmd shell "/data/local/tmp/device-net-probe $app_uid 10.10.10.1 443" | \
  tee "$ARTIFACT_DIR/probe-internal.txt"
hdc_cmd shell "/data/local/tmp/device-net-probe $app_uid example.com 443" | \
  tee "$ARTIFACT_DIR/probe-dns-tcp.txt"
grep -q 'connected 10.10.10.1:443' "$ARTIFACT_DIR/probe-internal.txt"
grep -q 'connected example.com:443' "$ARTIFACT_DIR/probe-dns-tcp.txt"

echo "==> disconnect and reconnect with a new platform start transaction"
click 400 340
wait_layout_text '未连接|Disconnected' 30
wait_tun_absent 30
click 400 340
wait_layout_text '已连接|Connected' "$VPN_START_TIMEOUT"

capture_hilog "$ENTRY_HILOG_FILE" HOpenConnectEntry
attempt_count="$(sed -n 's/.*VPN start completed attempt \([^ ]*\).*/\1/p' "$ENTRY_HILOG_FILE" | \
  sort -u | wc -l | tr -d ' ')"
if [ "$attempt_count" -lt 2 ]; then
  echo "expected at least two distinct completed VPN start attempts, got $attempt_count" >&2
  exit 1
fi

hdc_cmd shell "ifconfig vpn-tun" >"$ARTIFACT_DIR/tun-reconnected.txt"
grep -Eq 'UP.*RUNNING|RUNNING.*UP' "$ARTIFACT_DIR/tun-reconnected.txt"

echo "ARM64 QEMU AnyConnect E2E OK"
echo "artifacts: $ARTIFACT_DIR"
