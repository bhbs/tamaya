set -eu
old={{old_path}}
new={{new_path}}
if [ -d "$new" ] && [ ! -e "$old" ]; then
  printf '%s\n' "$new"
else
  echo "runtime rename did not complete: $old -> $new" >&2
  exit 1
fi
