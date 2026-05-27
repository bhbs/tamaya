set -eu
xdg_data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
artifact_dir="$xdg_data_home/v/artifacts/{{app}}"
mkdir -p "$artifact_dir"
destination="$artifact_dir/artifact.tar"
tmp="$destination.tmp.$$"
cat > "$tmp"
mv "$tmp" "$destination"
printf '%s\n' "$destination"
