set -eu
{{metadata_helpers}}
progress "removing environment variable"
app={{app}}
key={{key}}
data_dir={{data}}
metadata="$data_dir/apps/$app/metadata.toml"
if test -f "$metadata"; then
  validate_metadata_file "$metadata" "$app"
  app_type="$md_app_type"
  test "$app_type" != "published" || { echo "$app is a published app and does not support environment variables" >&2; exit 1; }
fi
dest="/etc/tamaya/apps/$app.env"
umask 077
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
trap 'exit 1' HUP INT TERM
sudo test -f "$dest" || { echo "no environment variables set for $app" >&2; exit 1; }
sudo cat "$dest" | awk -v key="$key" 'index($0, key "=") == 1 { found=1 } END { exit !found }' ||
  { echo "key $key not found for $app" >&2; exit 1; }
sudo cat "$dest" | awk -v key="$key" 'index($0, key "=") != 1' > "$tmp"
sudo install -o root -g root -m 0600 "$tmp" "$dest.tmp"
sudo mv "$dest.tmp" "$dest"
progress "environment variable removed"
