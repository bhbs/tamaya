set -eu
{{metadata_helpers}}
progress "loading environment variables"
app={{app}}
data_dir={{data}}
metadata="$data_dir/apps/$app/metadata.toml"
if test -f "$metadata"; then
  validate_metadata_file "$metadata" "$app"
  app_type="$md_app_type"
  test "$app_type" != "published" || { echo "$app is a published app and does not support environment variables" >&2; exit 1; }
fi
dest="/etc/tamaya/apps/$app.env"
if sudo test -f "$dest"; then
  sudo cat "$dest" | awk -F= 'NF { print $1 }'
fi
