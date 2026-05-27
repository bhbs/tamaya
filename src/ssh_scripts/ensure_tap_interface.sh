set -eu
tap={{tap}}
if ! ip link show dev "$tap" >/dev/null 2>&1; then
  ip tuntap add dev "$tap" mode tap
fi
ip addr replace 10.0.0.1/30 dev "$tap"
ip link set "$tap" up
ip route replace 10.0.0.2/32 dev "$tap"
printf '%s\n' "$tap"
