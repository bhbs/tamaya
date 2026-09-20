{{prelude}}
test "$md_status" != "stopped" || {
  echo "$app is stopped; use deploy, publish, or rollback to resume it" >&2
  exit 1
}
progress "enabling maintenance route"
domain="$(value domain)"
path="$(value path)"
test -n "$domain" || { echo "$app has no domain" >&2; exit 1; }
domain_key_value="$(domain_key "$domain")"
maintenance_changed=false
cleanup() {
  cleanup_status=$?
  trap - EXIT
  cleanup_restored=true
  if test "$maintenance_changed" = true; then
    sudo cp -p "$maintenance_state_backup/metadata" "$metadata" || cleanup_restored=false
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
maintenance_changed=true
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
maintenance_changed=false
progress "maintenance route enabled"
