metadata_string() {
  sed -n "s/^$2 = \"\\(.*\\)\"$/\\1/p" "$1"
}

metadata_number() {
  sed -n "s/^$2 = \\([0-9][0-9]*\\)$/\\1/p" "$1"
}

metadata_corrupt() {
  echo "corrupted metadata in $1: $2" >&2
  return 1
}

metadata_validate_app_name() {
  case "$1" in
    ''|*[!A-Za-z0-9_-]*) return 1 ;;
  esac
}

metadata_validate_release() {
  case "$1" in
    ''|*[!0-9-]*|-*|*-) return 1 ;;
  esac
}

metadata_validate_domain() {
  metadata_domain="${1#http://}"
  test -n "$metadata_domain" || return 1
  test "${#metadata_domain}" -le 253 || return 1
  metadata_old_ifs="$IFS"
  IFS=.
  set -- $metadata_domain
  IFS="$metadata_old_ifs"
  for metadata_label in "$@"; do
    test -n "$metadata_label" && test "${#metadata_label}" -le 63 || return 1
    case "$metadata_label" in
      *[!A-Za-z0-9-]*|-*|*-) return 1 ;;
    esac
  done
}

metadata_validate_path() {
  case "$1" in
    /) return 0 ;;
    /*)
      case "$1" in
        */|*//*|*..*|*\?*|*\#*|*[!A-Za-z0-9/._~%-]*) return 1 ;;
      esac
      ;;
    *) return 1 ;;
  esac
}

metadata_validate_health_path() {
  case "$1" in
    /*)
      case "$1" in
        *[!A-Za-z0-9/._?=\&-]*) return 1 ;;
      esac
      ;;
    *) return 1 ;;
  esac
}

metadata_validate_uint() {
  case "$1" in
    ''|*[!0-9]*) return 1 ;;
  esac
}

validate_metadata_file() {
  metadata_file="$1"
  metadata_expected_app="${2:-}"
  test -f "$metadata_file" ||
    { metadata_corrupt "$metadata_file" "file is missing"; return 1; }

  md_app="$(metadata_string "$metadata_file" app)"
  md_current="$(metadata_string "$metadata_file" current)"
  md_previous="$(metadata_string "$metadata_file" previous)"
  md_app_type="$(metadata_string "$metadata_file" app_type)"
  md_unit="$(metadata_string "$metadata_file" unit)"
  md_port="$(metadata_number "$metadata_file" port)"
  md_domain="$(metadata_string "$metadata_file" domain)"
  md_path="$(metadata_string "$metadata_file" path)"
  md_route_kind="$(metadata_string "$metadata_file" route_kind)"
  md_status="$(metadata_string "$metadata_file" status)"
  md_health_path="$(metadata_string "$metadata_file" health_path)"
  md_health_retries="$(metadata_number "$metadata_file" health_retries)"
  md_health_timeout="$(metadata_number "$metadata_file" health_timeout)"
  md_health_interval="$(metadata_number "$metadata_file" health_interval)"
  md_publish_type="$(metadata_string "$metadata_file" publish_type)"
  md_site_dir="$(metadata_string "$metadata_file" site_dir)"

  test -n "$md_app_type" || md_app_type="process"
  if test -z "$md_route_kind"; then
    if test -z "$md_domain"; then md_route_kind="none"
    elif test -z "$md_path" || test "$md_path" = "/"; then md_route_kind="root"
    else md_route_kind="path"
    fi
  fi

  metadata_validate_app_name "$md_app" ||
    { metadata_corrupt "$metadata_file" "invalid app name"; return 1; }
  test -z "$metadata_expected_app" || test "$md_app" = "$metadata_expected_app" ||
    { metadata_corrupt "$metadata_file" "app name does not match its directory"; return 1; }
  metadata_validate_release "$md_current" ||
    { metadata_corrupt "$metadata_file" "invalid current release"; return 1; }
  test -z "$md_previous" || metadata_validate_release "$md_previous" ||
    { metadata_corrupt "$metadata_file" "invalid previous release"; return 1; }
  case "$md_app_type" in process|published) ;; *)
    metadata_corrupt "$metadata_file" "invalid app type"; return 1
  esac
  case "$md_status" in running|stopped|maintenance) ;; *)
    metadata_corrupt "$metadata_file" "invalid status"; return 1
  esac
  case "$md_route_kind" in none|root|path) ;; *)
    metadata_corrupt "$metadata_file" "invalid route kind"; return 1
  esac

  if test -z "$md_domain"; then
    test -z "$md_path" && test "$md_route_kind" = "none" ||
      { metadata_corrupt "$metadata_file" "route without a domain"; return 1; }
  else
    metadata_validate_domain "$md_domain" ||
      { metadata_corrupt "$metadata_file" "invalid domain"; return 1; }
    metadata_validate_path "$md_path" ||
      { metadata_corrupt "$metadata_file" "invalid route path"; return 1; }
    if test "$md_path" = "/"; then
      test "$md_route_kind" = "root" ||
        { metadata_corrupt "$metadata_file" "root path has inconsistent route kind"; return 1; }
    else
      test "$md_route_kind" = "path" ||
        { metadata_corrupt "$metadata_file" "path route has inconsistent route kind"; return 1; }
    fi
  fi

  metadata_validate_uint "$md_port" ||
    { metadata_corrupt "$metadata_file" "invalid port"; return 1; }
  if test "$md_app_type" = "process"; then
    test "$md_port" -ge 1 && test "$md_port" -le 65535 ||
      { metadata_corrupt "$metadata_file" "process port is out of range"; return 1; }
    test "$md_unit" = "tamaya-$md_app-$md_current.service" ||
      { metadata_corrupt "$metadata_file" "invalid systemd unit"; return 1; }
    metadata_validate_health_path "$md_health_path" ||
      { metadata_corrupt "$metadata_file" "invalid health path"; return 1; }
    test -n "$md_health_retries" || md_health_retries=5
    test -n "$md_health_timeout" || md_health_timeout=2
    test -n "$md_health_interval" || md_health_interval=1
    metadata_validate_uint "$md_health_retries" && test "$md_health_retries" -ge 1 ||
      { metadata_corrupt "$metadata_file" "invalid health retries"; return 1; }
    metadata_validate_uint "$md_health_timeout" && test "$md_health_timeout" -ge 1 ||
      { metadata_corrupt "$metadata_file" "invalid health timeout"; return 1; }
    metadata_validate_uint "$md_health_interval" ||
      { metadata_corrupt "$metadata_file" "invalid health interval"; return 1; }
    test -z "$md_publish_type" && test -z "$md_site_dir" ||
      { metadata_corrupt "$metadata_file" "process metadata contains publish fields"; return 1; }
  else
    test "$md_port" = "0" && test -z "$md_unit" && test -z "$md_health_path" ||
      { metadata_corrupt "$metadata_file" "published metadata contains process fields"; return 1; }
    case "$md_publish_type" in static|spa) ;; *)
      metadata_corrupt "$metadata_file" "invalid publish type"; return 1
    esac
    test "$md_site_dir" = "$(dirname "$metadata_file")/releases/$md_current/site" ||
      { metadata_corrupt "$metadata_file" "invalid published site directory"; return 1; }
    md_health_retries=0
    md_health_timeout=0
    md_health_interval=0
  fi
}

acquire_app_operation_lock() {
  sudo mkdir -p "$data_dir/app-locks"
  operation_lock="$data_dir/app-locks/$app.lock"
  sudo touch "$operation_lock"
  sudo chown "$(id -u):$(id -g)" "$operation_lock"
  sudo chmod 0600 "$operation_lock"
  exec 6>"$operation_lock"
  flock 6
}

atomic_write_metadata() {
  sudo sh -c '
    set -eu
    umask 077
    metadata_dir=$1
    metadata_target=$2
    metadata_tmp=$(mktemp "$metadata_dir/.metadata.toml.tmp.XXXXXX")
    trap '\''rm -f "$metadata_tmp"'\'' EXIT
    trap '\''exit 1'\'' HUP INT TERM
    cat >"$metadata_tmp"
    chown root:root "$metadata_tmp"
    chmod 0600 "$metadata_tmp"
    mv "$metadata_tmp" "$metadata_target"
    trap - EXIT HUP INT TERM
  ' sh "$app_dir" "$metadata"
}
