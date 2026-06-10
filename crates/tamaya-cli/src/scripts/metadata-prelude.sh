set -eu
app={{app}}
data_dir={{data}}
caddy_dir={{caddy}}
{{app_units}}
{{caddy_helpers}}
app_dir="$data_dir/apps/$app"
metadata="$app_dir/metadata.toml"
test -f "$metadata" || { echo "$app is not deployed" >&2; exit 1; }
acquire_app_operation_lock
validate_metadata_file "$metadata" "$app"
value() { metadata_string "$metadata" "$1"; }
