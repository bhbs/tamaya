validate_app_name() {
  case "$app" in
    ''|*[!a-zA-Z0-9_-]*)
      echo "invalid app name: ${app:-<empty>}" >&2
      return 1
      ;;
  esac
}

is_release_unit_file() {
  case "$1" in
    tamaya-"$app"-[0-9]*.service)
      case "$1" in
        *[!a-zA-Z0-9._-]*) return 1 ;;
      esac
      return 0
      ;;
    *) return 1 ;;
  esac
}

disable_release_units() {
  local remove_files=false unit prefix
  validate_app_name || exit 1
  if test "${1:-}" = "remove"; then
    remove_files=true
  fi
  prefix="tamaya-${app}-"
  systemctl list-unit-files "${prefix}"*.service --no-legend 2>/dev/null | awk '{print $1}' |
  while IFS= read -r unit || test -n "${unit:-}"; do
    test -n "$unit" || continue
    is_release_unit_file "$unit" || continue
    sudo systemctl disable --now "$unit" >/dev/null || true
    if $remove_files; then
      sudo rm -f "/etc/systemd/system/$unit"
    fi
  done
}
