set -eu
host={{host}}
port={{port}}
if command -v nc >/dev/null 2>&1; then
  nc -z -w 5 "$host" "$port"
elif command -v bash >/dev/null 2>&1 && bash -c "timeout 5 bash -c '</dev/tcp/$host/$port' 2>/dev/null"; then
  :
elif command -v curl >/dev/null 2>&1; then
  curl -sf --max-time 5 "http://$host:$port" >/dev/null
else
  echo "no tool available for health check (nc, bash TCP, or curl)" >&2
  exit 1
fi
