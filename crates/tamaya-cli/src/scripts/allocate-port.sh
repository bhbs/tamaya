port=""
for candidate in $(seq {{start}} {{end}}); do
  if ! ss -H -ltn | awk '{print $4}' | grep -Eq "[:.]$candidate$"; then port="$candidate"; break; fi
done
test -n "$port" || { echo "no free Tamaya port" >&2; exit 1; }
