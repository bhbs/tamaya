set -eu
old={{old_path}}
new={{new_path}}
if [ ! -d "$old" ]; then
  echo "source directory does not exist: $old" >&2
  exit 1
fi
if [ -d "$new" ]; then
  rm -rf "$new"
fi
mkdir -p "$(dirname "$new")"
mv "$old" "$new"
printf '%s\n' "$new"
