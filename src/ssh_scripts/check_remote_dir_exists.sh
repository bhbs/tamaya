set -eu
dir={{path}}
if [ -d "$dir" ]; then
  printf 'exists\n'
else
  printf 'missing\n'
fi
