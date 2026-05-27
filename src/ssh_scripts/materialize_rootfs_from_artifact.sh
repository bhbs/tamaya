set -eu
app="{{app}}"
artifact={{artifact_path}}
rootfs_size_mib={{rootfs_size_mib}}
data_size_mib={{data_size_mib}}
port={{port}}
init={{init}}

xdg_data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
xdg_state_home="${XDG_STATE_HOME:-$HOME/.local/state}"
data_root="$xdg_data_home/v"
app_dir="$data_root/apps/$app"
work_dir="$xdg_state_home/v/build/$app/rootfs"

rootfs="$app_dir/rootfs.ext4"
data="$app_dir/data.ext4"
config="$app_dir/config.json"
metadata="$app_dir/metadata.json"

command -v tar >/dev/null
command -v truncate >/dev/null
mkfs_bin="$(command -v mkfs.ext4 || command -v mke2fs)"

rm -rf "$work_dir"
mkdir -p "$work_dir" "$app_dir"
tar -C "$work_dir" -xf "$artifact" >&2

truncate -s "${rootfs_size_mib}M" "$rootfs"
"$mkfs_bin" -q -F -d "$work_dir" "$rootfs" >&2

if [ ! -f "$data" ]; then
  truncate -s "${data_size_mib}M" "$data"
  "$mkfs_bin" -q -F "$data" >&2
fi

printf '{"app":"%s","rootfs":"%s","data":"%s","port":%s,"init":"%s"}\n' \
  "$app" "$rootfs" "$data" "$port" "$init" > "$config"
printf '{"app":"%s","artifact":"%s","rootfs":"%s","data":"%s"}\n' \
  "$app" "$artifact" "$rootfs" "$data" > "$metadata"

rm -rf "$work_dir"

printf '%s\n' "$rootfs"
printf '%s\n' "$data"
printf '%s\n' "$config"
printf '%s\n' "$metadata"
