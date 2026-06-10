set -eu
progress "loading app data"
app={{app}}
data_dir={{data}}
caddy_dir={{caddy}}
{{app_units}}
{{caddy_shared}}
app_dir="$data_dir/apps/$app"
test -d "$app_dir" || { echo "$app is not deployed" >&2; exit 1; }
metadata="$app_dir/metadata.toml"
acquire_app_operation_lock
if test -f "$metadata"; then validate_metadata_file "$metadata" "$app"; fi
value() { metadata_string "$metadata" "$1"; }
deleting_app=true
progress "removing Caddy route"
{{remove}}
progress "removing release services"
disable_release_units remove
sudo systemctl daemon-reload
sudo rm -f "/etc/tamaya/apps/$app.env"
{{delete_data}}
progress "app deleted"
printf 'deleted %s\n' "$app"
