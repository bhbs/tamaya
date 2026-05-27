set -eu
api_socket={{api_socket_path}}
response="$(curl -sS -i --unix-socket "$api_socket" \
  -X {{method}} \
  -H 'Accept: application/json' \
  -H 'Content-Type: application/json' \
  --data {{body}} \
  {{url}})"
status="$(printf '%s\n' "$response" | sed -n '1s/^HTTP\/[0-9.]* \([0-9][0-9][0-9]\).*/\1/p')"
case "$status" in
  2??) exit 0 ;;
  *)
    printf '%s\n' "$response" >&2
    exit 1
    ;;
esac
