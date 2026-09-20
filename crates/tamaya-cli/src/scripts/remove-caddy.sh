domain="${domain:-$(value domain 2>/dev/null || true)}"
path="${path:-$(value path 2>/dev/null || true)}"
remove_backup="$(mktemp -d)"
remove_changed=false
remove_maintenance=false
remove_committed=false
remove_cleanup() {
  remove_status=$?
  trap - EXIT
  remove_restored=true
  if test "$remove_changed" = true && test "$remove_committed" = false; then
    if test -f "$remove_backup/metadata"; then
      sudo cp -p "$remove_backup/metadata" "$metadata.tmp" && sudo mv "$metadata.tmp" "$metadata" || remove_restored=false
    fi
    if test -f "$remove_backup/route"; then
      sudo cp -p "$remove_backup/route" "$route_dir/$app.caddy" || remove_restored=false
    else
      sudo rm -f "$route_dir/$app.caddy" || remove_restored=false
    fi
    if test "$remove_maintenance" = true; then
      caddy_restore_maintenance_state || remove_restored=false
    fi
    if test "$remove_restored" = true && test -n "$domain"; then
      rebuild_domain "$domain" || remove_restored=false
    fi
    # Restore a legacy site after rebuilding; migration removes legacy files.
    if test "$remove_restored" = true && test -f "$remove_backup/legacy"; then
      (
        flock 7 || exit 1
        sudo cp -p "$remove_backup/legacy" "$caddy_dir/$app.caddy" || exit 1
        if command -v caddy >/dev/null 2>&1; then
          sudo caddy validate --config /etc/caddy/Caddyfile || exit 1
        fi
        sudo systemctl reload caddy
      ) 7>"$lock_dir/caddy.lock" || remove_restored=false
    fi
  fi
  if test "$remove_restored" = false; then
    echo "failed to restore $app; manual recovery required from $remove_backup ${maintenance_state_backup:-}" >&2
    exit "$remove_status"
  fi
  caddy_discard_maintenance_backup || true
  sudo rm -rf "$remove_backup" || true
  exit "$remove_status"
}
remove_caddy_commit() {
  remove_committed=true
  if test "$remove_maintenance" = true; then
    sudo rm -rf "$data_dir/static/maintenance/$(domain_key "$domain")" || true
  fi
}
trap remove_cleanup EXIT
trap 'exit 1' HUP INT TERM
if test -f "$metadata"; then sudo cp -p "$metadata" "$remove_backup/metadata"; fi
if test -f "$route_dir/$app.caddy"; then sudo cp -p "$route_dir/$app.caddy" "$remove_backup/route"; fi
if test -f "$caddy_dir/$app.caddy"; then sudo cp -p "$caddy_dir/$app.caddy" "$remove_backup/legacy"; fi
if test -n "$domain" && test "${deleting_app:-false}" = true; then
  other_domain_app=false
  for other_metadata in "$data_dir"/apps/*/metadata.toml; do
    test -f "$other_metadata" || continue
    other_expected_app="$(basename "$(dirname "$other_metadata")")"
    validate_metadata_file "$other_metadata" "$other_expected_app"
    other_app="$(metadata_string "$other_metadata" app)"
    test "$other_app" != "$app" || continue
    other_domain="$(metadata_string "$other_metadata" domain)"
    test "$other_domain" = "$domain" || continue
    other_domain_app=true
    break
  done
  if test "$other_domain_app" = false; then
    caddy_backup_maintenance_state "$domain"
    remove_maintenance=true
  fi
fi
remove_changed=true
if test -n "$domain"; then
  sudo rm -f "$route_dir/$app.caddy" "$route_dir/$app.caddy.tmp"
  sudo rm -f "$caddy_dir/$app.caddy" "$caddy_dir/$app.caddy.tmp"
  if test "$remove_maintenance" = true; then
    sudo rm -f "$domain_dir/$(domain_key "$domain").maintenance"
  fi
  rebuild_domain "$domain"
else
  sudo rm -f "$caddy_dir/$app.caddy" "$caddy_dir/$app.caddy.tmp"
  sudo systemctl reload caddy
fi
