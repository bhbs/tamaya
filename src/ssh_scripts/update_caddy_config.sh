set -eu
app={{app}}
domain={{domain}}
target={{target}}
config_dir={{config_dir}}
config_file="$config_dir/$app.caddy"
sudo mkdir -p "$config_dir"
sudo tee "$config_file" >/dev/null <<'EOF'
{{domain_block}} {
    reverse_proxy {{vm_host}}:{{vm_port}}
}
EOF
printf '%s\n' "$config_file"
