set -eu
xdg_data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
xdg_state_home="${XDG_STATE_HOME:-$HOME/.local/state}"
if [ -n "${XDG_RUNTIME_DIR:-}" ]; then
  runtime_root="$XDG_RUNTIME_DIR/v"
else
  runtime_root="$xdg_state_home/v/runtime"
fi
data_root="$xdg_data_home/v"
state_root="$xdg_state_home/v"
runtime_dir="$runtime_root/{{app}}"
log_dir="$runtime_dir/logs"
mkdir -p "$data_root/images" "$data_root/volumes" "$state_root" "$runtime_dir" "$log_dir"
printf '%s\n' "$runtime_dir"
