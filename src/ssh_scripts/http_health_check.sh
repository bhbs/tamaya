set -eu
host={{host}}
port={{port}}
path=/{{path_clean}}
timeout={{timeout}}
if ! command -v curl >/dev/null 2>&1; then
  echo "ERROR: curl is required for HTTP health checks" >&2
  exit 1
fi
curl_output=$(curl -sS --max-time "$timeout" -w '\n%{http_code}' "http://$host:$port$path" 2>&1) || {
  echo "ERROR: health check failed for http://$host:$port$path" >&2
  echo "$curl_output" >&2
  exit 1
}
http_code=$(echo "$curl_output" | tail -1)
if [ "$http_code" -lt 200 ] 2>/dev/null || [ "$http_code" -ge 400 ] 2>/dev/null; then
  echo "ERROR: health check returned HTTP $http_code for http://$host:$port$path" >&2
  echo "$curl_output" | head -n -1 >&2
  exit 1
fi
