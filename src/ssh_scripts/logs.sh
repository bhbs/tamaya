set -eu
log_dir={{log_dir}}
if [ -d "$log_dir" ]; then
  for log in "$log_dir"/*.log; do
    if [ -f "$log" ]; then
      printf '=== %s ===\n' "$(basename "$log")"
      cat "$log"
    fi
  done
fi
