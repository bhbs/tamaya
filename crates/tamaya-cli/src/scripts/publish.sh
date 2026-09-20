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
route_had_previous=false
legacy_had_previous=false
metadata_switched=false
metadata_had_previous=false
links_switched=false
current_link=""
previous_link=""
for link_name in current previous; do
  if test -e "$app_dir/$link_name" && test ! -L "$app_dir/$link_name"; then
    echo "$app $link_name must be a symbolic link" >&2
    exit 1
  fi
done
if test -L "$app_dir/current"; then current_link="$(readlink "$app_dir/current")"; fi
if test -L "$app_dir/previous"; then previous_link="$(readlink "$app_dir/previous")"; fi
cleanup() {
  cleanup_status=$?
  trap - EXIT
  recovery_failed=false
  # Restore the source state before rebuilding the public route or deleting
  # the failed release, including the absence of state on a first deployment.
  if test "$metadata_switched" = true; then
    if test "$metadata_had_previous" = true; then
      sudo mv "$app_dir/metadata.toml.bak" "$metadata" || recovery_failed=true
    else
      sudo rm -f "$metadata" || recovery_failed=true
    fi
  fi
  if test "$links_switched" = true; then
    if test -n "$current_link"; then
      sudo ln -sfn "$current_link" "$app_dir/current" || recovery_failed=true
    else
      sudo rm -f "$app_dir/current" || recovery_failed=true
    fi
    if test -n "$previous_link"; then
      sudo ln -sfn "$previous_link" "$app_dir/previous" || recovery_failed=true
    else
      sudo rm -f "$app_dir/previous" || recovery_failed=true
    fi
  fi
  if test "$caddy_switched" = true; then
    if test "$route_had_previous" = true; then
      sudo mv "$route_dir/$app.caddy.bak" "$route_dir/$app.caddy" || recovery_failed=true
    else
      sudo rm -f "$route_dir/$app.caddy" || recovery_failed=true
    fi
    if test "$recovery_failed" = false; then
      rebuild_domain "$domain" || recovery_failed=true
    fi
    if test "$recovery_failed" = false && test "$legacy_had_previous" = true; then
      # A worker upgraded from standalone Caddy files may not have had a
      # route snippet. Restore its original file after rebuilding the domain.
      if ! (
        flock 7 || exit 1
        sudo mv "$app_dir/legacy.caddy.bak" "$caddy_dir/$app.caddy" || exit 1
        if command -v caddy >/dev/null 2>&1; then
          sudo caddy validate --config /etc/caddy/Caddyfile || exit 1
        fi
        sudo systemctl reload caddy
      ) 7>"$lock_dir/caddy.lock"; then
        recovery_failed=true
      fi
    fi
  fi
  if test "$recovery_failed" = true; then
    echo "failed to restore $app; keeping release $release for manual recovery" >&2
    exit "$cleanup_status"
  fi
  sudo rm -rf "$staging" "$app_dir/releases/$release" || true
  exit "$cleanup_status"
}
trap cleanup EXIT
sudo mkdir -p "$staging/site"
progress "uploading site files"
sudo tar -xf - -C "$staging/site"
sudo find "$staging/site" -type d -exec chmod 0755 {} +
sudo find "$staging/site" -type f -exec chmod 0644 {} +
sudo chown -R root:root "$staging/site"
sudo mv "$staging" "$app_dir/releases/$release"
progress "recording release metadata"
sudo rm -f "$app_dir/metadata.toml.bak"
if test -f "$metadata"; then
  sudo cp "$metadata" "$app_dir/metadata.toml.bak"
  metadata_had_previous=true
fi
metadata_switched=true
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
progress "switching Caddy route"
sudo rm -f "$route_dir/$app.caddy.bak"
if test -f "$route_dir/$app.caddy"; then
  sudo cp "$route_dir/$app.caddy" "$route_dir/$app.caddy.bak"
  route_had_previous=true
fi
sudo rm -f "$app_dir/legacy.caddy.bak"
if test -f "$caddy_dir/$app.caddy"; then
  sudo cp "$caddy_dir/$app.caddy" "$app_dir/legacy.caddy.bak"
  legacy_had_previous=true
fi
caddy_switched=true
caddy_write_published_route_snippet "$app" "$metadata_path" "$site_dir" "$publish_type"
sudo rm -f "$caddy_dir/$app.caddy" "$caddy_dir/$app.caddy.tmp"
rebuild_domain "$domain"
links_switched=true
sudo ln -sfn "releases/$release" "$app_dir/current"
if test -n "$old_release"; then sudo ln -sfn "releases/$old_release" "$app_dir/previous"; fi
trap - EXIT
ls -1dt "$app_dir"/releases/* 2>/dev/null | tail -n +6 | xargs -r sudo rm -rf
sudo rm -f "$caddy_dir/$app.caddy.bak" "$route_dir/$app.caddy.bak" "$app_dir/metadata.toml.bak" "$app_dir/legacy.caddy.bak" || true
progress "site published"
caddy_print_merged_domain_file "$domain"
printf 'published %s release %s\n' "$app" "$release"
