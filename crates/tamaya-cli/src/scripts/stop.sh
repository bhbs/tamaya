{{prelude}}
progress "removing Caddy route"
{{remove}}
progress "stopping release services"
disable_release_units
validate_metadata_file "$metadata" "$app"
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
status = "stopped"
health_path = "$md_health_path"
health_retries = $md_health_retries
health_timeout = $md_health_timeout
health_interval = $md_health_interval
publish_type = "$md_publish_type"
site_dir = "$md_site_dir"
EOF
printf 'stopped %s\n' "$app"
