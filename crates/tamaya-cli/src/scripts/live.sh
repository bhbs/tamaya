{{prelude}}
progress "restoring live route"
domain="$(value domain)"
path="$(value path)"
port="$md_port"
app_type="$(value app_type)"
test -n "$app_type" || app_type="process"
publish_type="$(value publish_type)"
site_dir="$(value site_dir)"
test -n "$domain" || { echo "$app has no domain" >&2; exit 1; }
metadata_path="$path"
if is_root_path "$path"; then metadata_path="/"; fi
domain_key_value="$(domain_key "$domain")"
sudo rm -f "$domain_dir/$domain_key_value.maintenance"
sudo rm -rf "$data_dir/static/maintenance/$domain_key_value"
if test "$app_type" = "published"; then
  caddy_write_published_route_snippet "$app" "$metadata_path" "$site_dir" "$publish_type"
else
  caddy_write_process_route_snippet "$app" "$metadata_path" "$port"
fi
atomic_write_metadata <<EOF
app = "$md_app"
current = "$md_current"
previous = "$md_previous"
app_type = "$md_app_type"
unit = "$md_unit"
port = $md_port
domain = "$md_domain"
path = "$md_path"
route_kind = "$md_route_kind"
status = "running"
health_path = "$md_health_path"
health_retries = $md_health_retries
health_timeout = $md_health_timeout
health_interval = $md_health_interval
publish_type = "$md_publish_type"
site_dir = "$md_site_dir"
EOF
rebuild_domain "$domain"
progress "live route restored"
