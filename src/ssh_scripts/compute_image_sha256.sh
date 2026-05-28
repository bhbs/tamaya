set -eu
xdg_data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
image_dir="$xdg_data_home/v/images"
destination="$image_dir/{{app}}-{{kind}}-{{filename}}"
if [ -f "$destination" ]; then
  sha256sum "$destination" | cut -d' ' -f1
else
  printf 'missing\n'
fi
printf '%s\n' "$destination"
