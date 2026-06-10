set -eu
progress "checking worker prerequisites"
data_dir={{data}}
caddy_dir={{caddy}}
command -v systemctl >/dev/null
if ! command -v ss >/dev/null || ! command -v flock >/dev/null || ! command -v curl >/dev/null || ! command -v caddy >/dev/null; then
  if command -v apt-get >/dev/null; then
    sudo apt-get update -qq
    sudo apt-get install -y -qq iproute2 util-linux curl caddy
  elif command -v dnf >/dev/null; then
    sudo dnf install -y iproute util-linux curl caddy
  elif command -v yum >/dev/null; then
    sudo yum install -y iproute util-linux curl caddy
  else
    echo "install ss, flock, curl, and caddy manually" >&2
    exit 1
  fi
fi
sudo mkdir -p "$data_dir/apps" "$data_dir/caddy-routes" "$data_dir/caddy-domains" "$data_dir/caddy-locks" "$caddy_dir" /etc/tamaya/apps
if ! sudo grep -Fq "import $caddy_dir/*.caddy" /etc/caddy/Caddyfile; then
  printf '\nimport %s/*.caddy\n' "$caddy_dir" | sudo tee -a /etc/caddy/Caddyfile >/dev/null
fi
sudo caddy validate --config /etc/caddy/Caddyfile
progress "starting Caddy"
sudo systemctl enable --now caddy
progress "worker setup complete"
printf 'worker ready\n'
