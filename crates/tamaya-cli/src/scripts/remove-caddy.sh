domain="${domain:-$(value domain 2>/dev/null || true)}"
path="${path:-$(value path 2>/dev/null || true)}"
if test -n "$domain"; then
  sudo rm -f "$route_dir/$app.caddy" "$route_dir/$app.caddy.tmp"
  sudo rm -f "$caddy_dir/$app.caddy" "$caddy_dir/$app.caddy.tmp"
  if test "${deleting_app:-false}" = true; then
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
      sudo rm -f "$domain_dir/$(domain_key "$domain").maintenance"
      sudo rm -rf "$data_dir/static/maintenance/$(domain_key "$domain")"
    fi
  fi
  rebuild_domain "$domain"
else
  sudo rm -f "$caddy_dir/$app.caddy" "$caddy_dir/$app.caddy.tmp"
  sudo systemctl reload caddy
fi
