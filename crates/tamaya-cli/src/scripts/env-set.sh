set -eu
{{metadata_helpers}}
progress "installing environment variable"
app={{app}}
key={{key}}
data_dir={{data}}
acquire_app_operation_lock
metadata="$data_dir/apps/$app/metadata.toml"
if test -f "$metadata"; then
  validate_metadata_file "$metadata" "$app"
  app_type="$md_app_type"
  test "$app_type" != "published" || { echo "$app is a published app and does not support environment variables" >&2; exit 1; }
fi
dest="/etc/tamaya/apps/$app.env"
umask 077
sudo mkdir -p /etc/tamaya/apps
tmp="$(mktemp "/etc/tamaya/apps/.$app.env.tmp.XXXXXX")"
trap 'rm -f "$tmp"' EXIT
trap 'exit 1' HUP INT TERM
# The controller supplies one quoted, escaped EnvironmentFile value on stdin.
# Put it before legacy rows, which may end in an unescaped continuation.
printf '%s=' "$key" > "$tmp"
cat >> "$tmp"
printf '\n' >> "$tmp"
if sudo test -f "$dest"; then
  sudo awk -v key="$key" 'index($0, key "=") != 1' "$dest" >> "$tmp"
fi
sudo chown root:root "$tmp"
sudo chmod 0600 "$tmp"
sudo mv "$tmp" "$dest"
progress "environment variable installed"
