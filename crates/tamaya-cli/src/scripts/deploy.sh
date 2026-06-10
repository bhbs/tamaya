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
if test -f "$metadata"; then validate_metadata_file "$metadata" "$app"; fi
ensure_route_compatible "$app" "process" "$domain" "$path"
exec 8>"$data_dir/ports.lock"
flock 8
if ! id "tamaya-$app" >/dev/null 2>&1; then sudo useradd --system --home "$app_dir/data" --shell /usr/sbin/nologin "tamaya-$app"; fi
sudo chown -R "tamaya-$app":"tamaya-$app" "$app_dir/data"
release="$(date -u +%Y%m%d%H%M%S)"
while test -e "$app_dir/releases/$release"; do release="${release}-1"; done
staging="$app_dir/releases/.${release}.tmp"
caddy_switched=false
metadata_switched=false
cleanup() {
  sudo systemctl disable --now "tamaya-$app-$release.service" >/dev/null 2>&1 || true
  sudo rm -f "/etc/systemd/system/tamaya-$app-$release.service"
  sudo systemctl daemon-reload
  sudo systemctl reset-failed "tamaya-$app-$release.service" >/dev/null 2>&1 || true
  sudo rm -rf "$staging" "$app_dir/releases/$release"
  if test "$caddy_switched" = true; then
    if test -n "$domain"; then
      sudo mv "$route_dir/$app.caddy.bak" "$route_dir/$app.caddy" 2>/dev/null || sudo rm -f "$route_dir/$app.caddy"
      rebuild_domain "$domain" || true
    fi
  fi
  if test "$metadata_switched" = true; then
    sudo mv "$app_dir/metadata.toml.bak" "$app_dir/metadata.toml" 2>/dev/null || sudo rm -f "$app_dir/metadata.toml"
  fi
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
old_unit="${md_unit:-}"
old_release="${md_current:-}"
if test -z "$domain"; then domain="${md_domain:-}"; fi
if test -z "$path"; then path="${md_path:-}"; fi
if test -z "$route_kind"; then route_kind="$(route_kind_from_metadata "$domain" "$path")"; fi
metadata_path="$path"
if test -n "$domain" && is_root_path "$path"; then metadata_path="/"; fi
progress "recording release metadata"
sudo ln -sfn "releases/$release" "$app_dir/current"
if test -n "$old_release"; then sudo ln -sfn "releases/$old_release" "$app_dir/previous"; fi
sudo rm -f "$app_dir/metadata.toml.bak"
sudo cp "$app_dir/metadata.toml" "$app_dir/metadata.toml.bak" 2>/dev/null || true
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
metadata_switched=true
if test -n "$domain"; then
  progress "switching Caddy route"
  sudo rm -f "$route_dir/$app.caddy.bak"
  sudo cp "$route_dir/$app.caddy" "$route_dir/$app.caddy.bak" 2>/dev/null || true
  caddy_write_process_route_snippet "$app" "$metadata_path" "$port"
  sudo rm -f "$caddy_dir/$app.caddy" "$caddy_dir/$app.caddy.tmp"
  caddy_switched=true
  rebuild_domain "$domain"
fi
if test -n "$old_unit" && test "$old_unit" != "$unit"; then
  progress "stopping previous release"
  sudo systemctl disable --now "$old_unit" >/dev/null || true
fi
ls -1dt "$app_dir"/releases/* 2>/dev/null | tail -n +6 | xargs -r sudo rm -rf
sudo rm -f "$caddy_dir/$app.caddy.bak"
sudo rm -f "$route_dir/$app.caddy.bak" "$app_dir/metadata.toml.bak"
metadata_switched=false
trap - EXIT
progress "release deployed"
if test -n "$domain"; then
  caddy_print_merged_domain_file "$domain"
fi
printf 'deployed %s release %s on port %s\n' "$app" "$release" "$port"
