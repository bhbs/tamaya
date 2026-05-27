set -eu
if ! command -v ip >/dev/null 2>&1; then
  echo "ip command not found" >&2
  exit 1
fi

ip tuntap show 2>/dev/null | while IFS=: read -r tap _; do
  case "$tap" in
    t-*|*-deploy) ;;
    *) continue ;;
  esac
{{preserve_pattern}}

  if ! ip link show dev "$tap" >/dev/null 2>&1; then
    continue
  fi

  state="$(ip -brief link show dev "$tap" | awk '{print $2}')"
  if [ "$state" != "DOWN" ]; then
    continue
  fi

  ip link set "$tap" down 2>/dev/null || true
  ip tuntap del dev "$tap" mode tap
  printf '%s\n' "$tap"
done
