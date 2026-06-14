route_dir="$data_dir/caddy-routes"
domain_dir="$data_dir/caddy-domains"
lock_dir="$data_dir/caddy-locks"
caddy_routes_dir="$route_dir"
caddy_domains_dir="$domain_dir"
caddy_locks_dir="$lock_dir"
sudo mkdir -p "$route_dir" "$domain_dir" "$lock_dir" "$caddy_dir"

domain_key() {
  case "$1" in
    http://*) key="http_${1#http://}" ;;
    *) key="$1" ;;
  esac
  printf '%s' "$key" | sed 's/[^A-Za-z0-9._-]/_/g'
}

caddy_metadata_value() {
  metadata_string "$1" "$2"
}

is_root_path() {
  case "$1" in
    ""|"/") return 0 ;;
    *) return 1 ;;
  esac
}

route_path_key() {
  if is_root_path "$1"; then
    printf '/'
  else
    printf '%s' "$1" | sed 's#/*$##'
  fi
}

route_kind_from_metadata() {
  rk_domain="$1"
  rk_path="$2"
  test -n "$rk_domain" || { printf 'none'; return; }
  if is_root_path "$rk_path"; then
    printf 'root'
  else
    printf 'path'
  fi
}

route_kind() {
  route_kind_from_metadata "$1" "$2"
}

ensure_route_compatible() {
  check_app="$1"
  check_app_type="$2"
  check_domain="$3"
  check_path="$4"

  check_metadata="$data_dir/apps/$check_app/metadata.toml"
  if test -f "$check_metadata"; then
    validate_metadata_file "$check_metadata" "$check_app"
    current_type="$(caddy_metadata_value "$check_metadata" app_type)"
    test -n "$current_type" || current_type="process"
    if test "$current_type" != "$check_app_type"; then
      echo "$check_app is already deployed as $current_type; delete it before deploying as $check_app_type" >&2
      return 1
    fi
  fi

  test -n "$check_domain" || return 0
  check_key="$(route_path_key "$check_path")"
  for check_other_metadata in "$data_dir"/apps/*/metadata.toml; do
    test -f "$check_other_metadata" || continue
    check_other_expected_app="$(basename "$(dirname "$check_other_metadata")")"
    validate_metadata_file "$check_other_metadata" "$check_other_expected_app"
    check_other_app="$(caddy_metadata_value "$check_other_metadata" app)"
    test -n "$check_other_app" || check_other_app="$(basename "$(dirname "$check_other_metadata")")"
    test "$check_other_app" = "$check_app" && continue
    check_other_domain="$(caddy_metadata_value "$check_other_metadata" domain)"
    test "$check_other_domain" = "$check_domain" || continue
    check_other_path="$(caddy_metadata_value "$check_other_metadata" path)"
    check_other_key="$(route_path_key "$check_other_path")"
    if is_root_path "$check_path" && is_root_path "$check_other_path"; then
      echo "$check_domain already has root route $check_other_app" >&2
      return 1
    fi
    if test "$check_key" = "$check_other_key"; then
      echo "$check_domain$check_key already has path route $check_other_app" >&2
      return 1
    fi
  done
}

caddy_restore_domain_file() {
  restore_out="$1"
  restore_bak="$2"
  restore_had_previous="$3"
  if test "$restore_had_previous" = true; then
    sudo mv "$restore_bak" "$restore_out"
  else
    sudo rm -f "$restore_out" "$restore_bak"
  fi
  sudo systemctl reload caddy >/dev/null 2>&1 || true
}

caddy_validate_and_reload_domain() {
  validate_out="$1"
  validate_bak="$2"
  validate_had_previous="$3"
  if command -v caddy >/dev/null 2>&1 && ! sudo caddy validate --config /etc/caddy/Caddyfile; then
    caddy_restore_domain_file "$validate_out" "$validate_bak" "$validate_had_previous"
    return 1
  fi
  if ! sudo systemctl reload caddy; then
    caddy_restore_domain_file "$validate_out" "$validate_bak" "$validate_had_previous"
    return 1
  fi
  sudo rm -f "$validate_bak"
}

caddy_replace_domain_file() {
  replace_out="$1"
  replace_tmp="$2"
  replace_bak="$replace_out.bak"
  replace_had_previous=false
  sudo rm -f "$replace_bak"
  if sudo test -f "$replace_out"; then
    sudo cp "$replace_out" "$replace_bak"
    replace_had_previous=true
  fi
  sudo chown root:root "$replace_tmp"
  sudo chmod 0644 "$replace_tmp"
  sudo mv "$replace_tmp" "$replace_out"
  caddy_validate_and_reload_domain "$replace_out" "$replace_bak" "$replace_had_previous"
}

caddy_remove_domain_file() {
  remove_out="$1"
  remove_bak="$remove_out.bak"
  remove_had_previous=false
  sudo rm -f "$remove_bak"
  if sudo test -f "$remove_out"; then
    sudo cp "$remove_out" "$remove_bak"
    remove_had_previous=true
  fi
  sudo rm -f "$remove_out"
  caddy_validate_and_reload_domain "$remove_out" "$remove_bak" "$remove_had_previous"
}

caddy_write_process_route_snippet() {
  write_app="$1"
  write_path="$2"
  write_port="$3"
  test -n "$write_path" || return 0
  sudo mkdir -p "$route_dir"
  if is_root_path "$write_path"; then
    sudo tee "$route_dir/$write_app.caddy.tmp" >/dev/null <<EOF
handle {
    reverse_proxy 127.0.0.1:$write_port
}
EOF
  else
    write_matcher="@tamaya_$(printf '%s' "$write_app" | sed 's/[^A-Za-z0-9_]/_/g')"
    write_match="$write_path $write_path/*"
    sudo tee "$route_dir/$write_app.caddy.tmp" >/dev/null <<EOF
$write_matcher path $write_match
handle $write_matcher {
    reverse_proxy 127.0.0.1:$write_port
}
EOF
  fi
  sudo mv "$route_dir/$write_app.caddy.tmp" "$route_dir/$write_app.caddy"
}

caddy_write_published_route_snippet() {
  write_app="$1"
  write_path="$2"
  write_site_dir="$3"
  write_publish_type="$4"
  test -n "$write_path" || return 0
  sudo mkdir -p "$route_dir"
  if test "$write_publish_type" = "spa"; then
    write_try_files='    try_files {path} /index.html'
  else
    write_try_files='    try_files {path} {path}.html {path}/ /404.html'
  fi
  if is_root_path "$write_path"; then
    {
      printf 'handle {\n'
      printf '    root * %s\n' "$write_site_dir"
      printf '%s\n' "$write_try_files"
      printf '    file_server\n'
      printf '}\n'
    } | sudo tee "$route_dir/$write_app.caddy.tmp" >/dev/null
  else
    write_matcher="@tamaya_$(printf '%s' "$write_app" | sed 's/[^A-Za-z0-9_]/_/g')"
    write_match="$write_path $write_path/*"
    {
      printf '%s path %s\n' "$write_matcher" "$write_match"
      printf 'handle %s {\n' "$write_matcher"
      printf '    root * %s\n' "$write_site_dir"
      printf '%s\n' "$write_try_files"
      printf '    file_server\n'
      printf '}\n'
    } | sudo tee "$route_dir/$write_app.caddy.tmp" >/dev/null
  fi
  sudo mv "$route_dir/$write_app.caddy.tmp" "$route_dir/$write_app.caddy"
}

caddy_site_block_header() {
  header_file="$1"
  sudo sed -n '1s/^[[:space:]]*//;1s/[[:space:]]*{[[:space:]]*$//;1p' "$header_file" 2>/dev/null || true
}

caddy_remove_stale_standalone_files_for_domain() {
  remove_domain="$1"
  remove_key="$(domain_key "$remove_domain")"
  remove_merged="$caddy_dir/$remove_key.caddy"
  for remove_metadata in "$data_dir"/apps/*/metadata.toml; do
    test -f "$remove_metadata" || continue
    remove_expected_app="$(basename "$(dirname "$remove_metadata")")"
    validate_metadata_file "$remove_metadata" "$remove_expected_app"
    remove_app="$(caddy_metadata_value "$remove_metadata" app)"
    remove_route_domain="$(caddy_metadata_value "$remove_metadata" domain)"
    test "$remove_route_domain" = "$remove_domain" || continue
    test -n "$remove_app" || remove_app="$(basename "$(dirname "$remove_metadata")")"
    sudo rm -f "$caddy_dir/$remove_app.caddy" "$caddy_dir/$remove_app.caddy.tmp" "$caddy_dir/$remove_app.caddy.bak"
  done
  for stale_file in "$caddy_dir"/*.caddy; do
    test -f "$stale_file" || continue
    test "$stale_file" = "$remove_merged" && continue
    stale_header="$(caddy_site_block_header "$stale_file")"
    test "$stale_header" = "$remove_domain" || continue
    sudo rm -f "$stale_file" "$stale_file.tmp" "$stale_file.bak"
  done
}

rebuild_domain() (
  rebuild_domain_value="$1"
  test -n "$rebuild_domain_value" || return 0
  rebuild_key="$(domain_key "$rebuild_domain_value")"
  rebuild_target="$caddy_dir/$rebuild_key.caddy"
  rebuild_tmp="$rebuild_target.tmp"
  rebuild_maintenance="$domain_dir/$rebuild_key.maintenance"
  umask 077
  rebuild_path_list="$(mktemp)"
  trap 'rm -f "$rebuild_path_list"' EXIT
  trap 'exit 1' HUP INT TERM
  rebuild_root_app=""
  rebuild_root_count=0
  sudo touch "$lock_dir/caddy.lock"
  sudo chown "$(id -u):$(id -g)" "$lock_dir/caddy.lock"
  sudo chmod 0600 "$lock_dir/caddy.lock"

  (
    flock 7

    caddy_remove_stale_standalone_files_for_domain "$rebuild_domain_value"

    if sudo test -f "$rebuild_maintenance"; then
      rebuild_static="$data_dir/static/maintenance/$rebuild_key"
      sudo mkdir -p "$rebuild_static"
      rebuild_message="$(sudo cat "$rebuild_maintenance")"
      printf '%s\n' "$rebuild_message" | sudo sed "s#__DOMAIN__#$rebuild_domain_value#g" | sudo tee "$rebuild_static/index.html" >/dev/null
      {
        printf '%s {\n' "$rebuild_domain_value"
        printf '    root * %s\n' "$rebuild_static"
        printf '    file_server\n'
        printf '}\n'
      } | sudo tee "$rebuild_tmp" >/dev/null
      caddy_replace_domain_file "$rebuild_target" "$rebuild_tmp"
      rm -f "$rebuild_path_list"
      exit 0
    fi

    for rebuild_metadata in "$data_dir"/apps/*/metadata.toml; do
      test -f "$rebuild_metadata" || continue
      rebuild_expected_app="$(basename "$(dirname "$rebuild_metadata")")"
      validate_metadata_file "$rebuild_metadata" "$rebuild_expected_app"
      rebuild_app="$(caddy_metadata_value "$rebuild_metadata" app)"
      rebuild_route_domain="$(caddy_metadata_value "$rebuild_metadata" domain)"
      rebuild_path="$(caddy_metadata_value "$rebuild_metadata" path)"
      rebuild_status="$(caddy_metadata_value "$rebuild_metadata" status)"
      test "$rebuild_route_domain" = "$rebuild_domain_value" || continue
      test "$rebuild_status" != "stopped" || continue
      test -n "$rebuild_app" || rebuild_app="$(basename "$(dirname "$rebuild_metadata")")"
      test -f "$route_dir/$rebuild_app.caddy" || continue
      if is_root_path "$rebuild_path"; then
        rebuild_root_count=$((rebuild_root_count + 1))
        rebuild_root_app="$rebuild_app"
      else
        rebuild_sort_key="$(route_path_key "$rebuild_path")"
        printf '%s\t%s\t%s\n' "${#rebuild_sort_key}" "$rebuild_sort_key" "$rebuild_app" >>"$rebuild_path_list"
      fi
    done

    if test "$rebuild_root_count" -gt 1; then
      echo "domain $rebuild_domain_value has multiple root routes" >&2
      rm -f "$rebuild_path_list"
      exit 1
    fi

    if test -s "$rebuild_path_list"; then
      rebuild_prev_key=""
      sort -rn -k1,1 "$rebuild_path_list" | while IFS="$(printf '\t')" read -r _ rebuild_sort_key rebuild_app; do
        test "$rebuild_sort_key" != "$rebuild_prev_key" || {
          echo "$rebuild_domain_value$rebuild_sort_key has duplicate path routes" >&2
          exit 1
        }
        rebuild_prev_key="$rebuild_sort_key"
      done || {
        rm -f "$rebuild_path_list"
        exit 1
      }
    fi

    if test "$rebuild_root_count" = 0 && test ! -s "$rebuild_path_list"; then
      caddy_remove_domain_file "$rebuild_target"
      rm -f "$rebuild_path_list"
      exit 0
    fi

    {
      printf '%s {\n' "$rebuild_domain_value"
      if test -s "$rebuild_path_list"; then
        sort -rn -k1,1 "$rebuild_path_list" | while IFS="$(printf '\t')" read -r _ _ rebuild_app; do
          sed 's/^/    /' "$route_dir/$rebuild_app.caddy"
        done
      fi
      if test "$rebuild_root_count" = 1; then
        sed 's/^/    /' "$route_dir/$rebuild_root_app.caddy"
      fi
      printf '}\n'
    } | sudo tee "$rebuild_tmp" >/dev/null
    caddy_replace_domain_file "$rebuild_target" "$rebuild_tmp"
    rm -f "$rebuild_path_list"
  ) 7>"$lock_dir/caddy.lock"
)

caddy_rebuild_domain() {
  rebuild_domain "$1"
}

caddy_route_kind() {
  case "$1" in
    published)
      case "$2" in
        spa) printf 'spa' ;;
        *) printf 'static' ;;
      esac
      ;;
    *)
      printf 'process' ;;
  esac
}

caddy_print_domain_routes() (
  print_domain="$1"
  test -n "$print_domain" || return 0
  print_key="$(domain_key "$print_domain")"
  print_maintenance="$domain_dir/$print_key.maintenance"
  umask 077
  print_paths="$(mktemp)"
  print_routes="$(mktemp)"
  trap 'rm -f "$print_paths" "$print_routes"' EXIT
  trap 'exit 1' HUP INT TERM
  print_root_app=""
  print_root_path="/"
  print_root_release=""
  print_root_app_type=""
  print_root_publish_type=""
  print_root_count=0

  printf '\nRoutes (%s)\n\n' "$print_domain"

  if sudo test -f "$print_maintenance"; then
    printf '* : maintenance\n\n'
    rm -f "$print_paths" "$print_routes"
    return 0
  fi

  for print_metadata in "$data_dir"/apps/*/metadata.toml; do
    test -f "$print_metadata" || continue
    print_expected_app="$(basename "$(dirname "$print_metadata")")"
    validate_metadata_file "$print_metadata" "$print_expected_app"
    print_app="$(caddy_metadata_value "$print_metadata" app)"
    print_route_domain="$(caddy_metadata_value "$print_metadata" domain)"
    print_path="$(caddy_metadata_value "$print_metadata" path)"
    print_status="$(caddy_metadata_value "$print_metadata" status)"
    print_app_type="$(caddy_metadata_value "$print_metadata" app_type)"
    print_publish_type="$(caddy_metadata_value "$print_metadata" publish_type)"
    print_release="$(caddy_metadata_value "$print_metadata" current)"
    test "$print_route_domain" = "$print_domain" || continue
    test "$print_status" != "stopped" || continue
    test -n "$print_app" || print_app="$(basename "$(dirname "$print_metadata")")"
    test -n "$print_app_type" || print_app_type="process"
    test -f "$route_dir/$print_app.caddy" || continue
    if is_root_path "$print_path"; then
      print_root_count=$((print_root_count + 1))
      print_root_app="$print_app"
      print_root_path="/"
      print_root_release="$print_release"
      print_root_app_type="$print_app_type"
      print_root_publish_type="$print_publish_type"
    else
      print_sort_key="$(route_path_key "$print_path")"
      printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "${#print_sort_key}" "$print_sort_key" "$print_path" "$print_app" \
        "$print_app_type" "$print_publish_type" "$print_release" >>"$print_paths"
    fi
  done

  if test -s "$print_paths"; then
    sort -rn -k1,1 "$print_paths" | while IFS="$(printf '\t')" read -r _ _ print_path print_app print_app_type print_publish_type print_release; do
      printf '%s\t%s\t%s\t%s\t%s\n' \
        "$print_path" "$print_app" "$print_app_type" "$print_publish_type" "$print_release"
    done >"$print_routes"
  fi
  if test "$print_root_count" = 1; then
    printf '%s\t%s\t%s\t%s\t%s\n' \
      "$print_root_path" "$print_root_app" "$print_root_app_type" \
      "$print_root_publish_type" "$print_root_release" >>"$print_routes"
  fi

  if test ! -s "$print_routes"; then
    printf '  (no active routes)\n\n'
    rm -f "$print_paths" "$print_routes"
    return 0
  fi

  while IFS="$(printf '\t')" read -r print_path print_app print_app_type print_publish_type _print_release; do
    print_kind="$(caddy_route_kind "$print_app_type" "$print_publish_type")"
    printf '%s : %s : %s\n' "$print_path" "$print_kind" "$print_app"
  done <"$print_routes"

  rm -f "$print_paths" "$print_routes"
  printf '\n'
)

caddy_print_merged_domain_file() {
  caddy_print_domain_routes "$1"
}
