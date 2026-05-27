set -eu
pid={{pid}}
if kill -0 "$pid" 2>/dev/null; then
  printf 'running\n'
else
  printf 'dead\n'
fi
