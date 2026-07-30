# Dev AnyConnect-compatible headend for H-OpenConnect.
FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    ocserv \
    gnutls-bin \
    iptables \
    iproute2 \
    ca-certificates \
  && rm -rf /var/lib/apt/lists/* \
  && mkdir -p /var/run/ocserv /etc/ocserv

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 443/tcp 443/udp
ENTRYPOINT ["/entrypoint.sh"]
