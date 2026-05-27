FROM node:26-bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends busybox-static ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /app /data /proc /sys /dev /run \
    && printf '%s\n' \
        "const http = require('node:http');" \
        "" \
        "const host = '0.0.0.0';" \
        "const port = 3000;" \
        "" \
        "const server = http.createServer((req, res) => {" \
        "  if (req.url === '/health') {" \
        "    res.writeHead(200, { 'content-type': 'text/plain; charset=utf-8' });" \
        "    res.end('ok\\n');" \
        "    return;" \
        "  }" \
        "" \
        "  res.writeHead(200, { 'content-type': 'application/json; charset=utf-8' });" \
        "  res.end(JSON.stringify({" \
        "    app: 'demo'," \
        "    runtime: 'node'," \
        "    message: 'Hello from v demo'," \
        "    path: req.url" \
        "  }, null, 2) + '\\n');" \
        "});" \
        "" \
        "server.listen(port, host, () => {" \
        "  console.log('demo app listening on http://' + host + ':' + port);" \
        "});" \
      > /app/server.js \
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
        'exec /usr/local/bin/node /app/server.js' \
      > /sbin/init \
    && chmod 0755 /sbin/init
