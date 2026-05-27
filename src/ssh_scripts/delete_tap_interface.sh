set -eu
tap={{tap}}
if ip link show dev "$tap" >/dev/null 2>&1; then
  ip link set "$tap" down 2>/dev/null || true
  ip tuntap del dev "$tap" mode tap
fi
