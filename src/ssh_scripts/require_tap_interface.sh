set -eu
tap={{tap}}
ip link show dev "$tap" >/dev/null
ip link show dev "$tap" | grep -q '<[^>]*UP'
printf '%s\n' "$tap"
