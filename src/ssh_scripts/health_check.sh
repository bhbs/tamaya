set -eu
host={{host}}
port={{port}}
timeout={{timeout}}
if command -v nc >/dev/null 2>&1; then
  nc -z -w "$timeout" "$host" "$port"
elif command -v bash >/dev/null 2>&1 && bash -c "timeout $timeout bash -c '</dev/tcp/$host/$port' 2>/dev/null"; then
  :
elif command -v curl >/dev/null 2>&1; then
  curl_response=$(curl -sS --max-time "$timeout" -o /dev/null -w '%{http_code}' "http://$host:$port" 2>&1) || {
    echo "ERROR: curl TCP check failed: $curl_response" >&2
    exit 1
  }
  if [ "$curl_response" -ge 200 ] 2>/dev/null && [ "$curl_response" -lt 400 ] 2>/dev/null; then
    exit 0
  fi
  echo "ERROR: curl TCP check returned HTTP $curl_response" >&2
  exit 1
else
  echo "ERROR: no tool available for health check (nc, bash TCP, or curl)" >&2
  exit 1
fi
