#!/usr/bin/env sh
# Local AnyConnect-compatible VPN headend (ocserv) for H-OpenConnect device tests.
#
# Usage:
#   ./scripts/dev-ocserv.sh start     # build image if needed, start, print client fields
#   ./scripts/dev-ocserv.sh stop
#   ./scripts/dev-ocserv.sh status
#   ./scripts/dev-ocserv.sh logs
#   ./scripts/dev-ocserv.sh url
#   ./scripts/dev-ocserv.sh restart
#   ./scripts/dev-ocserv.sh rebuild   # force rebuild image
#
# Defaults:
#   OCSERV_PORT=4433  OCSERV_USER=demo  OCSERV_PASS=demo
#
# Phone fills LAN IP (not 127.0.0.1), turns OFF strict cert trust.

set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
CTX_DIR="$ROOT_DIR/scripts/dev-ocserv"
DATA_DIR="${OCSERV_DATA_DIR:-$ROOT_DIR/.dev-ocserv}"
CONTAINER_NAME="${OCSERV_NAME:-hopenconnect-ocserv}"
IMAGE_NAME="${OCSERV_IMAGE_NAME:-hopenconnect-ocserv:local}"
HOST_PORT="${OCSERV_PORT:-4433}"
USER_NAME="${OCSERV_USER:-demo}"
USER_PASS="${OCSERV_PASS:-demo}"

die() {
  echo "dev-ocserv: $*" >&2
  exit 1
}

need_docker() {
  command -v docker >/dev/null 2>&1 || die "docker not found; install Docker Desktop"
  docker info >/dev/null 2>&1 || die "docker daemon not running"
}

lan_ip() {
  if [ -n "${OCSERV_HOST_IP:-}" ]; then
    printf '%s\n' "$OCSERV_HOST_IP"
    return
  fi
  for iface in en0 en1 en2 en3; do
    ip=$(ipconfig getifaddr "$iface" 2>/dev/null || true)
    if [ -n "$ip" ]; then
      printf '%s\n' "$ip"
      return
    fi
  done
  if command -v hostname >/dev/null 2>&1; then
    hostname -I 2>/dev/null | awk '{print $1; exit}'
  fi
}

server_url() {
  ip=$(lan_ip)
  if [ -z "$ip" ]; then
    ip="127.0.0.1"
  fi
  printf 'https://%s:%s\n' "$ip" "$HOST_PORT"
}

print_client_hints() {
  url=$(server_url)
  cat <<EOF

============================================
 H-OpenConnect -> local ocserv
============================================
  Server URL : ${url}
  Username   : ${USER_NAME}
  Password   : ${USER_PASS}
  Group      : (leave empty)

  App settings for self-signed cert:
    - Strict certificate trust     -> OFF
    - Block untrusted servers      -> OFF

  Notes:
    - Phone and Mac on the same LAN (or Mac hotspot).
    - Do NOT use 127.0.0.1 on the phone.
    - Hilog: anyconnect_obtain_cookie / anyconnect_cstp /
      anyconnect_setup_tun_fd / anyconnect_mainloop
============================================

EOF
}

write_conf() {
  cat >"$DATA_DIR/ocserv.conf" <<'CONF'
auth = "plain[passwd=/etc/ocserv/ocpasswd]"
tcp-port = 443
udp-port = 443
run-as-user = nobody
run-as-group = nogroup
socket-file = /var/run/ocserv/ocserv-socket

server-cert = /etc/ocserv/server-cert.pem
server-key = /etc/ocserv/server-key.pem
ca-cert = /etc/ocserv/server-cert.pem

isolate-workers = false
max-clients = 8
max-same-clients = 4
keepalive = 32400
dpd = 90
mobile-dpd = 1800
switch-to-tcp-timeout = 25
try-mtu-discovery = true

cisco-client-compat = true
dtls-legacy = true
tls-priorities = "NORMAL:%SERVER_PRECEDENCE:%COMPAT:-VERS-SSL3.0"

auth-timeout = 240
idle-timeout = 1200
mobile-idle-timeout = 2400
min-reauth-time = 0
max-ban-score = 0
cookie-timeout = 300
deny-roaming = false
rekey-time = 172800
rekey-method = ssl

use-occtl = true
pid-file = /var/run/ocserv/ocserv.pid
device = vpns
predictable-ips = true

ipv4-network = 10.10.10.0
ipv4-netmask = 255.255.255.0
dns = 1.1.1.1
dns = 8.8.8.8
tunnel-all-dns = true

mtu = 1400
route = default
no-route = 192.168.0.0/16
no-route = 10.0.0.0/8
no-route = 172.16.0.0/12
CONF
}

ensure_image() {
  if docker image inspect "$IMAGE_NAME" >/dev/null 2>&1; then
    return 0
  fi
  echo "dev-ocserv: building image ${IMAGE_NAME} (first time)..."
  docker build -t "$IMAGE_NAME" "$CTX_DIR"
}

cmd_rebuild() {
  need_docker
  echo "dev-ocserv: rebuilding ${IMAGE_NAME}..."
  docker build --no-cache -t "$IMAGE_NAME" "$CTX_DIR"
  echo "dev-ocserv: image ready"
}

prepare_data() {
  need_docker
  mkdir -p "$DATA_DIR/run"
  write_conf

  if [ ! -f "$DATA_DIR/server-cert.pem" ] || [ ! -f "$DATA_DIR/server-key.pem" ]; then
    echo "dev-ocserv: generating self-signed TLS certificate..."
    command -v openssl >/dev/null 2>&1 || die "openssl not found"
    openssl req -x509 -newkey rsa:2048 -nodes \
      -keyout "$DATA_DIR/server-key.pem" \
      -out "$DATA_DIR/server-cert.pem" \
      -days 3650 \
      -subj "/CN=hopenconnect-ocserv-dev/O=H-OpenConnect/C=CN" \
      >/dev/null 2>&1
    chmod 644 "$DATA_DIR/server-cert.pem" "$DATA_DIR/server-key.pem"
  fi

  echo "dev-ocserv: writing ocpasswd for user '${USER_NAME}'..."
  ensure_image
  docker run --rm --entrypoint bash \
    -v "$DATA_DIR:/data" \
    "$IMAGE_NAME" \
    -c "
      set -euo pipefail
      rm -f /data/ocpasswd
      printf '%s\n%s\n' '${USER_PASS}' '${USER_PASS}' | ocpasswd -c /data/ocpasswd '${USER_NAME}'
      chmod 644 /data/ocpasswd
    "
}

is_running() {
  docker ps --format '{{.Names}}' 2>/dev/null | grep -qx "$CONTAINER_NAME"
}

wait_ready() {
  i=0
  while [ "$i" -lt 30 ]; do
    if is_running; then
      if docker logs "$CONTAINER_NAME" 2>&1 | grep -Eqi 'listening|ocserv\[|main:|initialization'; then
        return 0
      fi
      if command -v nc >/dev/null 2>&1 && nc -z 127.0.0.1 "$HOST_PORT" 2>/dev/null; then
        # Port open is enough if process is ocserv
        if docker top "$CONTAINER_NAME" 2>/dev/null | grep -q ocserv; then
          return 0
        fi
      fi
    else
      # exited
      return 1
    fi
    i=$((i + 1))
    sleep 0.5
  done
  is_running
}

cmd_start() {
  need_docker
  if is_running; then
    echo "dev-ocserv: already running (${CONTAINER_NAME})"
    print_client_hints
    return 0
  fi

  docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
  ensure_image
  prepare_data

  echo "dev-ocserv: starting ${CONTAINER_NAME} on TCP/UDP ${HOST_PORT}..."
  docker run -d \
    --name "$CONTAINER_NAME" \
    --restart unless-stopped \
    --cap-add NET_ADMIN \
    --device /dev/net/tun \
    -p "${HOST_PORT}:443" \
    -p "${HOST_PORT}:443/udp" \
    -v "$DATA_DIR:/etc/ocserv" \
    -v "$DATA_DIR/run:/var/run/ocserv" \
    "$IMAGE_NAME" \
    >/dev/null

  if ! wait_ready; then
    echo "dev-ocserv: failed to become ready; logs:" >&2
    docker logs --tail 50 "$CONTAINER_NAME" 2>&1 || true
    die "start failed"
  fi

  echo "dev-ocserv: up"
  print_client_hints
}

cmd_stop() {
  need_docker
  if is_running || docker ps -a --format '{{.Names}}' | grep -qx "$CONTAINER_NAME"; then
    docker rm -f "$CONTAINER_NAME" >/dev/null
    echo "dev-ocserv: stopped"
  else
    echo "dev-ocserv: not running"
  fi
}

cmd_status() {
  need_docker
  if is_running; then
    echo "dev-ocserv: running"
    docker ps --filter "name=^/${CONTAINER_NAME}$" --format 'table {{.Names}}\t{{.Status}}\t{{.Ports}}'
    print_client_hints
  else
    echo "dev-ocserv: not running"
    exit 1
  fi
}

cmd_logs() {
  need_docker
  docker logs --tail "${LINES:-100}" -f "$CONTAINER_NAME"
}

cmd_url() {
  server_url
}

cmd_restart() {
  cmd_stop || true
  cmd_start
}

usage() {
  cat <<USAGE
Usage: scripts/dev-ocserv.sh <command>

Commands:
  start     Build image if needed, start ocserv, print phone client fields
  stop      Stop and remove the container
  restart   Stop then start
  rebuild   Force rebuild the local Docker image
  status    Show container + client fields
  logs      Follow container logs
  url       Print https://<lan-ip>:<port>

Environment:
  OCSERV_PORT         host port (default 4433)
  OCSERV_USER         username (default demo)
  OCSERV_PASS         password (default demo)
  OCSERV_HOST_IP      force advertised LAN IP
  OCSERV_NAME         container name
  OCSERV_DATA_DIR     state dir (default .dev-ocserv/)
  OCSERV_IMAGE_NAME   image tag (default hopenconnect-ocserv:local)
USAGE
}

main() {
  cmd=${1:-}
  case "$cmd" in
    start) cmd_start ;;
    stop) cmd_stop ;;
    restart) cmd_restart ;;
    rebuild) cmd_rebuild ;;
    status) cmd_status ;;
    logs) cmd_logs ;;
    url) cmd_url ;;
    -h|--help|help|"") usage; [ -n "$cmd" ] || exit 0 ;;
    *) die "unknown command: $cmd (try --help)" ;;
  esac
}

main "$@"
