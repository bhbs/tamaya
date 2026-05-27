set -eu
name={{name}}
input={{path}}
case "$input" in
  '$XDG_DATA_HOME/'*)
    suffix="${input#\$XDG_DATA_HOME/}"
    base="${XDG_DATA_HOME:-$HOME/.local/share}"
    resolved="$base/$suffix"
    ;;
  '$XDG_STATE_HOME/'*)
    suffix="${input#\$XDG_STATE_HOME/}"
    base="${XDG_STATE_HOME:-$HOME/.local/state}"
    resolved="$base/$suffix"
    ;;
  '$XDG_RUNTIME_DIR/'*)
    suffix="${input#\$XDG_RUNTIME_DIR/}"
    if [ -z "${XDG_RUNTIME_DIR:-}" ]; then
      echo "$name uses XDG_RUNTIME_DIR but it is not set" >&2
      exit 1
    fi
    resolved="$XDG_RUNTIME_DIR/$suffix"
    ;;
  /*)
    resolved="$input"
    ;;
  *)
    echo "$name must be an absolute path or start with \$XDG_DATA_HOME/, \$XDG_STATE_HOME/, or \$XDG_RUNTIME_DIR/" >&2
    exit 1
    ;;
esac
[ -f "$resolved" ] || { echo "$name does not exist or is not a file: $resolved" >&2; exit 1; }
[ -r "$resolved" ] || { echo "$name is not readable: $resolved" >&2; exit 1; }
printf '%s\n' "$resolved"
