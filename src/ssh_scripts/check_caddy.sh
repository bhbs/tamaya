set -eu
if command -v caddy >/dev/null 2>&1; then
  if systemctl is-active --quiet caddy 2>/dev/null; then
    printf '%s\n' "caddy: installed and running"
    exit 0
  fi
  caddy version >/dev/null 2>&1 && exit 0
fi
echo "caddy: not found or not running" >&2
exit 1
