set -eu
pid={{pid}}
runtime_dir={{remote_runtime_dir}}
if kill -0 "$pid" 2>/dev/null; then
  kill -TERM "$pid" 2>/dev/null || true
fi
i=0
while [ "$i" -lt 100 ]; do
  if ! kill -0 "$pid" 2>/dev/null; then
    break
  fi
  i=$((i + 1))
  sleep 0.025
done
if kill -0 "$pid" 2>/dev/null; then
  kill -KILL "$pid" 2>/dev/null || true
fi
rm -rf "$runtime_dir"
