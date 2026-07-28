#!/bin/bash
set -euo pipefail

mkdir -p /var/run/ocserv

# Tunnel clients (10.10.10.0/24) can reach outside when route=default.
iptables -t nat -C POSTROUTING -s 10.10.10.0/24 -j MASQUERADE 2>/dev/null \
  || iptables -t nat -A POSTROUTING -s 10.10.10.0/24 -j MASQUERADE 2>/dev/null \
  || true

# Drop privileges to nobody; key must be world-readable in this dev setup.
if [ -f /etc/ocserv/server-key.pem ]; then
  chmod 644 /etc/ocserv/server-key.pem || true
fi
if [ -f /etc/ocserv/server-cert.pem ]; then
  chmod 644 /etc/ocserv/server-cert.pem || true
fi
if [ -f /etc/ocserv/ocpasswd ]; then
  chmod 644 /etc/ocserv/ocpasswd || true
fi

exec ocserv -c /etc/ocserv/ocserv.conf -f -d 1
