{{prelude}}
{{health_check_failure}}
progress "loading previous release"
current="$(value current)"
previous="$(value previous)"
domain="$(value domain)"
path="$(value path)"
route_kind="$(value route_kind)"
health_path="$(value health_path)"
health_retries="$(metadata_number "$metadata" health_retries)"
health_timeout="$(metadata_number "$metadata" health_timeout)"
health_interval="$(metadata_number "$metadata" health_interval)"
app_type="$(value app_type)"
test -n "$app_type" || app_type="process"
publish_type="$(value publish_type)"
test -n "$health_path" || health_path="/health"
test -n "$health_retries" || health_retries=5
test -n "$health_timeout" || health_timeout=2
test -n "$health_interval" || health_interval=1
test -n "$route_kind" || route_kind="$(route_kind_from_metadata "$domain" "$path")"
metadata_path="$path"
if test -n "$domain" && is_root_path "$path"; then metadata_path="/"; fi
test -n "$previous" || { echo "$app has no previous release" >&2; exit 1; }
old_unit="$md_unit"
candidate_started=false
caddy_switched=false
route_had_previous=false
legacy_had_previous=false
metadata_switched=false
links_switched=false
cleanup() {
  cleanup_status=$?
  trap - EXIT
  recovery_failed=false
  # Rebuild against the original state, especially when it was stopped.
  if test "$metadata_switched" = true; then
    sudo mv "$app_dir/metadata.toml.bak" "$metadata" || recovery_failed=true
  fi
  if test "$links_switched" = true; then
    sudo ln -sfn "releases/$current" "$app_dir/current" || recovery_failed=true
    sudo ln -sfn "releases/$previous" "$app_dir/previous" || recovery_failed=true
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
      # The standalone file may have been the old release's only public route.
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
    echo "failed to restore $app; keeping release $previous for manual recovery" >&2
    exit "$cleanup_status"
  fi
  if test "$candidate_started" = true; then
    sudo systemctl disable --now "$unit" >/dev/null 2>&1 || true
  fi
  exit "$cleanup_status"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
if test "$app_type" = "published"; then
  progress "switching published release"
  previous_site_dir="$app_dir/releases/$previous/site"
  test -d "$previous_site_dir" || { echo "$app previous published release is missing site files" >&2; exit 1; }
  sudo cp "$metadata" "$app_dir/metadata.toml.bak"
  metadata_switched=true
  atomic_write_metadata <<EOF
app = "$app"
current = "$previous"
previous = "$current"
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
site_dir = "$previous_site_dir"
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
    caddy_write_published_route_snippet "$app" "$metadata_path" "$previous_site_dir" "$publish_type"
    sudo rm -f "$caddy_dir/$app.caddy" "$caddy_dir/$app.caddy.tmp"
    rebuild_domain "$domain"
  fi
  links_switched=true
  sudo ln -sfn "releases/$previous" "$app_dir/current"
  sudo ln -sfn "releases/$current" "$app_dir/previous"
  trap - EXIT
  sudo rm -f "$caddy_dir/$app.caddy.bak" "$route_dir/$app.caddy.bak" "$app_dir/metadata.toml.bak" "$app_dir/legacy.caddy.bak" || true
  progress "rollback complete"
  printf 'rolled back %s to release %s\n' "$app" "$previous"
  exit 0
fi
exec 8>"$data_dir/ports.lock"
flock 8
{{allocation}}
unit="tamaya-$app-$previous.service"
sudo sed -e "s/^Environment=PORT=.*/Environment=PORT=$port/" "/etc/systemd/system/$unit" | sudo tee "/etc/systemd/system/$unit.tmp" >/dev/null
sudo mv "/etc/systemd/system/$unit.tmp" "/etc/systemd/system/$unit"
sudo systemctl daemon-reload
progress "starting previous release"
candidate_started=true
sudo systemctl enable --now "$unit"
ok=false
progress "waiting for health check"
for _ in $(seq 1 "$health_retries"); do
  if curl -fsS --max-time "$health_timeout" "http://127.0.0.1:$port$health_path" >/dev/null; then ok=true; break; fi
  sleep "$health_interval"
done
test "$ok" = true || { report_health_check_failure "$unit" "127.0.0.1:$port$health_path"; exit 1; }
sudo cp "$metadata" "$app_dir/metadata.toml.bak"
metadata_switched=true
atomic_write_metadata <<EOF
app = "$app"
current = "$previous"
previous = "$current"
app_type = "process"
unit = "$unit"
port = $port
domain = "$domain"
path = "$metadata_path"
route_kind = "$route_kind"
status = "running"
health_path = "$health_path"
health_retries = $health_retries
health_timeout = $health_timeout
health_interval = $health_interval
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
sudo ln -sfn "releases/$previous" "$app_dir/current"
sudo ln -sfn "releases/$current" "$app_dir/previous"
trap - EXIT
test "$old_unit" = "$unit" || sudo systemctl disable --now "$old_unit" >/dev/null || true
sudo rm -f "$caddy_dir/$app.caddy.bak" "$route_dir/$app.caddy.bak" "$app_dir/metadata.toml.bak" "$app_dir/legacy.caddy.bak" || true
progress "rollback complete"
printf 'rolled back %s to release %s on port %s\n' "$app" "$previous" "$port"
