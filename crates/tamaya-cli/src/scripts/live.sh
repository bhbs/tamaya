{{prelude}}
test "$md_status" != "stopped" || {
  echo "$app is stopped; use deploy, publish, or rollback to resume it" >&2
  exit 1
}
progress "restoring live route"
domain="$(value domain)"
path="$(value path)"
port="$md_port"
app_type="$(value app_type)"
test -n "$app_type" || app_type="process"
publish_type="$(value publish_type)"
site_dir="$(value site_dir)"
test -n "$domain" || { echo "$app has no domain" >&2; exit 1; }
if test "$app_type" = "process" && ! sudo systemctl is-active --quiet "$md_unit"; then
  echo "$app current release service is not active: $md_unit" >&2
  exit 1
fi
metadata_path="$path"
if is_root_path "$path"; then metadata_path="/"; fi
domain_key_value="$(domain_key "$domain")"
live_changed=false
cleanup() {
  cleanup_status=$?
  trap - EXIT
  cleanup_restored=true
  if test "$live_changed" = true; then
    sudo cp -p "$maintenance_state_backup/metadata" "$metadata" || cleanup_restored=false
    if sudo test -f "$maintenance_state_backup/route"; then
      sudo cp -p "$maintenance_state_backup/route" "$route_dir/$app.caddy" || cleanup_restored=false
    else
      sudo rm -f "$route_dir/$app.caddy" || cleanup_restored=false
    fi
    caddy_restore_maintenance_state || cleanup_restored=false
    if test "$cleanup_restored" = true; then
      rebuild_domain "$domain" || cleanup_restored=false
    fi
  fi
  if test "$cleanup_restored" = true; then
    caddy_discard_maintenance_backup || true
  else
    echo "failed to restore $domain; manual recovery is required (backup: $maintenance_state_backup)" >&2
  fi
  exit "$cleanup_status"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
caddy_backup_maintenance_state "$domain"
sudo cp -p "$metadata" "$maintenance_state_backup/metadata"
if sudo test -f "$route_dir/$app.caddy"; then
  sudo cp -p "$route_dir/$app.caddy" "$maintenance_state_backup/route"
fi
live_changed=true
sudo rm -f "$domain_dir/$domain_key_value.maintenance"
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
live_changed=false
sudo rm -rf "$data_dir/static/maintenance/$domain_key_value"
progress "live route restored"
