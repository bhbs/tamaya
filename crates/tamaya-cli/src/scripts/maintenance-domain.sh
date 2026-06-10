set -eu
progress "enabling domain maintenance"
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
sudo tee "$domain_dir/$domain_key_value.maintenance" >/dev/null
rebuild_domain "$domain"
progress "domain maintenance enabled"
