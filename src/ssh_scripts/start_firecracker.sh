set -eu
firecracker_bin={{firecracker_bin}}
api_socket={{api_socket_path}}
log_dir={{log_dir}}
runtime_dir="$(dirname "$api_socket")"
mkdir -p "$runtime_dir" "$log_dir"
rm -f "$api_socket"
nohup "$firecracker_bin" --api-sock "$api_socket" > "$log_dir/firecracker.stdout.log" 2> "$log_dir/firecracker.stderr.log" < /dev/null &
pid=$!
i=0
while [ "$i" -lt 200 ]; do
  if [ -S "$api_socket" ]; then
    printf '%s\n' "$pid"
    exit 0
  fi
  if ! kill -0 "$pid" 2>/dev/null; then
    echo "Firecracker exited before creating API socket" >&2
    exit 1
  fi
  i=$((i + 1))
  sleep 0.025
done
kill "$pid" 2>/dev/null || true
echo "timed out waiting for Firecracker API socket: $api_socket" >&2
exit 1
