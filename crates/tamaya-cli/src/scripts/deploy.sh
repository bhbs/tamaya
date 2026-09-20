set -eu
progress "preparing release directories"
app={{app}}
domain={{domain}}
path={{path}}
route_kind={{route_kind}}
health={{health}}
data_dir={{data}}
caddy_dir={{caddy}}
{{caddy_shared}}
{{health_check_failure}}
app_dir="$data_dir/apps/$app"
sudo mkdir -p "$app_dir/releases" "$app_dir/data" "$caddy_dir"
metadata="$app_dir/metadata.toml"
acquire_app_operation_lock
old_unit=""
old_release=""
if test -f "$metadata"; then
  validate_metadata_file "$metadata" "$app"
  old_unit="$md_unit"
  old_release="$md_current"
  if test -z "$domain"; then domain="$md_domain"; fi
  if test -z "$path"; then path="$md_path"; fi
fi
# Resolve the effective route before checking other apps, whose metadata
# validation overwrites md_*. The controller cannot resolve inherited routes.
if test -n "$domain" && is_root_path "$path"; then path="/"; fi
route_kind="$(route_kind_from_metadata "$domain" "$path")"
metadata_path="$path"
ensure_route_compatible "$app" "process" "$domain" "$path"
exec 8>"$data_dir/ports.lock"
flock 8
if ! id "tamaya-$app" >/dev/null 2>&1; then sudo useradd --system --home "$app_dir/data" --shell /usr/sbin/nologin "tamaya-$app"; fi
sudo chown -R "tamaya-$app":"tamaya-$app" "$app_dir/data"
release="$(date -u +%Y%m%d%H%M%S)"
while test -e "$app_dir/releases/$release"; do release="${release}-1"; done
staging="$app_dir/releases/.${release}.tmp"
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
  sudo systemctl disable --now "tamaya-$app-$release.service" >/dev/null 2>&1 || true
  sudo rm -f "/etc/systemd/system/tamaya-$app-$release.service" || true
  sudo systemctl daemon-reload || true
  sudo systemctl reset-failed "tamaya-$app-$release.service" >/dev/null 2>&1 || true
  sudo rm -rf "$staging" "$app_dir/releases/$release" || true
  exit "$cleanup_status"
}
trap cleanup EXIT
sudo mkdir -p "$staging"
progress "uploading release binary"
sudo tee "$staging/app" >/dev/null
sudo chmod 0755 "$staging/app"
binary="$staging/app"
{{verify_binary_deps}}
sudo mv "$staging" "$app_dir/releases/$release"
{{writable_release_setup}}
{{allocation}}
unit="tamaya-$app-$release.service"
progress "installing systemd service"
sudo tee "/etc/systemd/system/$unit" >/dev/null <<EOF
{{unit_body}}EOF
sudo systemctl daemon-reload
progress "starting release service"
sudo systemctl enable --now "$unit"
ok=false
progress "waiting for health check"
for _ in $(seq 1 {{retries}}); do
  if curl -fsS --max-time {{timeout}} "http://127.0.0.1:$port$health" >/dev/null; then ok=true; break; fi
  sleep {{interval}}
done
test "$ok" = true || { report_health_check_failure "$unit" "127.0.0.1:$port$health"; exit 1; }
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
app_type = "process"
unit = "$unit"
port = $port
domain = "$domain"
path = "$metadata_path"
route_kind = "$route_kind"
status = "running"
health_path = "$health"
health_retries = {{retries}}
health_timeout = {{timeout}}
health_interval = {{interval}}
publish_type = ""
site_dir = ""
EOF
if test -n "$domain"; then
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
  caddy_write_process_route_snippet "$app" "$metadata_path" "$port"
  sudo rm -f "$caddy_dir/$app.caddy" "$caddy_dir/$app.caddy.tmp"
  rebuild_domain "$domain"
fi
links_switched=true
sudo ln -sfn "releases/$release" "$app_dir/current"
if test -n "$old_release"; then sudo ln -sfn "releases/$old_release" "$app_dir/previous"; fi
trap - EXIT
if test -n "$old_unit" && test "$old_unit" != "$unit"; then
  progress "stopping previous release"
  sudo systemctl disable --now "$old_unit" >/dev/null || true
fi
ls -1dt "$app_dir"/releases/* 2>/dev/null | tail -n +6 | xargs -r sudo rm -rf
sudo rm -f "$caddy_dir/$app.caddy.bak" "$route_dir/$app.caddy.bak" "$app_dir/metadata.toml.bak" "$app_dir/legacy.caddy.bak" || true
progress "release deployed"
if test -n "$domain"; then
  caddy_print_merged_domain_file "$domain"
fi
printf 'deployed %s release %s on port %s\n' "$app" "$release" "$port"
