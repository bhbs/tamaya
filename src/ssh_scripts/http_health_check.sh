set -eu
host={{host}}
port={{port}}
path=/{{path_clean}}
if command -v curl >/dev/null 2>&1; then
  curl -sf --max-time 10 "http://$host:$port$path" >/dev/null
else
  echo "curl is required for HTTP health checks" >&2
  exit 1
fi
