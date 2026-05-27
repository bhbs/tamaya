FROM --platform=linux/amd64 debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends busybox-static ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /www /data /proc /sys /dev /run \
    && printf 'demo' > /www/index.html \
    && printf '%s\n' \
        '#!/bin/busybox sh' \
        'set -eu' \
        '' \
        '/bin/busybox mount -t proc proc /proc 2>/dev/null || true' \
        '/bin/busybox mount -t sysfs sysfs /sys 2>/dev/null || true' \
        '/bin/busybox mount -t devtmpfs devtmpfs /dev 2>/dev/null || true' \
        '' \
        'i=0' \
        'while [ ! -d /sys/class/net/eth0 ] && [ "$i" -lt 100 ]; do' \
        '  i=$((i + 1))' \
        '  /bin/busybox sleep 0.05' \
        'done' \
        '' \
        '/bin/busybox ip link set lo up 2>/dev/null || true' \
        '/bin/busybox ip addr add 10.0.0.2/24 dev eth0 2>/dev/null || true' \
        '/bin/busybox ip link set eth0 up' \
        '' \
        'if [ -b /dev/vdb ]; then' \
        '  /bin/busybox mount -t ext4 /dev/vdb /data 2>/dev/null || true' \
        'fi' \
        '' \
        'exec /bin/busybox httpd -f -p 0.0.0.0:3000 -h /www' \
      > /sbin/init \
    && chmod 0755 /sbin/init
