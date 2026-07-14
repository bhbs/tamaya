set -eu
progress "preparing published release"
app={{app}}
domain={{domain}}
path={{path}}
route_kind={{route_kind}}
publish_type={{publish_type}}
data_dir={{data}}
caddy_dir={{caddy}}
{{caddy_shared}}
app_dir="$data_dir/apps/$app"
sudo mkdir -p "$app_dir/releases" "$caddy_dir"
metadata="$app_dir/metadata.toml"
acquire_app_operation_lock
old_release=""
if test -f "$metadata"; then
  validate_metadata_file "$metadata" "$app"
  old_release="$md_current"
fi
ensure_route_compatible "$app" "published" "$domain" "$path"
release="$(date -u +%Y%m%d%H%M%S)"
while test -e "$app_dir/releases/$release"; do release="${release}-1"; done
staging="$app_dir/releases/.${release}.tmp"
site_dir="$app_dir/releases/$release/site"
metadata_path="$path"
if is_root_path "$path"; then metadata_path="/"; fi
caddy_switched=false
metadata_switched=false
cleanup() {
  sudo rm -rf "$staging" "$app_dir/releases/$release"
  if test "$caddy_switched" = true; then
    sudo mv "$route_dir/$app.caddy.bak" "$route_dir/$app.caddy" 2>/dev/null || sudo rm -f "$route_dir/$app.caddy"
    rebuild_domain "$domain" || true
  fi
  if test "$metadata_switched" = true; then
    sudo mv "$app_dir/metadata.toml.bak" "$app_dir/metadata.toml" 2>/dev/null || sudo rm -f "$app_dir/metadata.toml"
  fi
}
trap cleanup EXIT
sudo mkdir -p "$staging/site"
progress "uploading site files"
sudo tar -xf - -C "$staging/site"
sudo find "$staging/site" -type d -exec chmod 0755 {} +
sudo find "$staging/site" -type f -exec chmod 0644 {} +
sudo chown -R root:root "$staging/site"
sudo mv "$staging" "$app_dir/releases/$release"
sudo ln -sfn "releases/$release" "$app_dir/current"
if test -n "$old_release"; then sudo ln -sfn "releases/$old_release" "$app_dir/previous"; fi
progress "recording release metadata"
sudo rm -f "$app_dir/metadata.toml.bak"
sudo cp "$app_dir/metadata.toml" "$app_dir/metadata.toml.bak" 2>/dev/null || true
atomic_write_metadata <<EOF
app = "$app"
current = "$release"
previous = "$old_release"
app_type = "published"
unit = ""
port = 0
domain = "$domain"
path = "$metadata_path"
route_kind = "$route_kind"
status = "running"
health_path = ""
health_retries = 0
health_timeout = 0
health_interval = 0
publish_type = "$publish_type"
site_dir = "$site_dir"
EOF
metadata_switched=true
progress "switching Caddy route"
sudo rm -f "$route_dir/$app.caddy.bak"
sudo cp "$route_dir/$app.caddy" "$route_dir/$app.caddy.bak" 2>/dev/null || true
caddy_write_published_route_snippet "$app" "$metadata_path" "$site_dir" "$publish_type"
sudo rm -f "$caddy_dir/$app.caddy" "$caddy_dir/$app.caddy.tmp"
caddy_switched=true
rebuild_domain "$domain"
ls -1dt "$app_dir"/releases/* 2>/dev/null | tail -n +6 | xargs -r sudo rm -rf
sudo rm -f "$caddy_dir/$app.caddy.bak"
sudo rm -f "$route_dir/$app.caddy.bak" "$app_dir/metadata.toml.bak"
metadata_switched=false
trap - EXIT
progress "site published"
caddy_print_merged_domain_file "$domain"
printf 'published %s release %s\n' "$app" "$release"
