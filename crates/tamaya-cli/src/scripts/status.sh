set -eu
{{metadata_helpers}}
progress "reading worker metadata"
{{filter}}
found=false
for app_dir in $apps; do
  test -f "$app_dir/metadata.toml" || continue
  found=true
  metadata="$app_dir/metadata.toml"
  expected_app="$(basename "$app_dir")"
  validate_metadata_file "$metadata" "$expected_app"
  value() { metadata_string "$metadata" "$1"; }
  app="$(value app)"; unit="$(value unit)"; release="$(value current)"; domain="$(value domain)"
  app_type="$(value app_type)"; test -n "$app_type" || app_type="process"
  path="$(value path)"; publish_type="$(value publish_type)"; site_dir="$(value site_dir)"
  route_kind="$(value route_kind)"
  if test -z "$route_kind"; then
    if test -z "$domain"; then
      route_kind="none"
    elif test -z "$path" || test "$path" = "/"; then
      route_kind="root"
    else
      route_kind="path"
    fi
  fi
  port="$md_port"
  kind="$app_type"
  if test "$app_type" = "published"; then
    kind="published/$publish_type"
    state="$(value status)"
    memory=""
    cpu=""
    port=""
  elif test -n "$unit"; then
    state="$(systemctl is-active "$unit" 2>/dev/null || true)"
    memory="$(systemctl show "$unit" -p MemoryCurrent --value 2>/dev/null || true)"
    cpu="$(systemctl show "$unit" -p CPUUsageNSec --value 2>/dev/null || true)"
  else
    state="$(value status)"
    memory=""
    cpu=""
  fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$app" "$state" "$release" "$port" "$domain" "$path" "$route_kind" "$kind" "$memory" "$cpu"
done
test "$found" = true || printf 'no apps\n'
progress "worker status loaded"
