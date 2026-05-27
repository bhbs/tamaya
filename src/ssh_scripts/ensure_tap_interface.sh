set -eu
tap={{tap}}
if ! ip link show dev "$tap" >/dev/null 2>&1; then
  ip tuntap add dev "$tap" mode tap
fi
ip link set "$tap" up
printf '%s\n' "$tap"
