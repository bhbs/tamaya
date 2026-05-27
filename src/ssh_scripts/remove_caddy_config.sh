set -eu
app={{app}}
config_dir={{config_dir}}
config_file="$config_dir/$app.caddy"
if [ -f "$config_file" ]; then
  sudo rm -f "$config_file"
  printf '%s\n' "$config_file"
else
  printf '%s\n' "no config to remove for $app"
fi
