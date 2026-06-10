{{prelude}}
progress "enabling maintenance route"
domain="$(value domain)"
path="$(value path)"
test -n "$domain" || { echo "$app has no domain" >&2; exit 1; }
domain_key_value="$(domain_key "$domain")"
sudo tee "$domain_dir/$domain_key_value.maintenance" >/dev/null
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
status = "maintenance"
health_path = "$md_health_path"
health_retries = $md_health_retries
health_timeout = $md_health_timeout
health_interval = $md_health_interval
publish_type = "$md_publish_type"
site_dir = "$md_site_dir"
EOF
rebuild_domain "$domain"
progress "maintenance route enabled"
