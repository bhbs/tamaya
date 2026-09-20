set -eu
progress "restoring domain routes"
domain={{domain}}
data_dir={{data}}
caddy_dir={{caddy}}
{{caddy_shared}}
known_domain=false
for metadata in "$data_dir"/apps/*/metadata.toml; do
  test -f "$metadata" || continue
  expected_app="$(basename "$(dirname "$metadata")")"
  validate_metadata_file "$metadata" "$expected_app"
  metadata_domain="$(caddy_metadata_value "$metadata" domain)"
  test "$metadata_domain" = "$domain" || continue
  known_domain=true
  break
done
test "$known_domain" = true || { echo "Tamaya has no known apps for $domain" >&2; exit 1; }
domain_key_value="$(domain_key "$domain")"
sudo test -f "$domain_dir/$domain_key_value.maintenance" || { echo "$domain is not in maintenance" >&2; exit 1; }
live_changed=false
cleanup() {
  cleanup_status=$?
  trap - EXIT
  cleanup_restored=true
  if test "$live_changed" = true; then
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
live_changed=true
sudo rm -f "$domain_dir/$domain_key_value.maintenance"
rebuild_domain "$domain"
live_changed=false
sudo rm -rf "$data_dir/static/maintenance/$domain_key_value"
progress "domain routes restored"
