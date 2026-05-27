set -eu
requested_firecracker_bin={{firecracker_bin}}
caddy_config_dir={{caddy_config_dir}}
case "$requested_firecracker_bin" in
  */*) firecracker_bin="$requested_firecracker_bin" ;;
  *)
    if command -v "$requested_firecracker_bin" >/dev/null 2>&1; then
      printf '%s\n' "firecracker already installed at $(command -v "$requested_firecracker_bin")"
      firecracker_bin="$(command -v "$requested_firecracker_bin")"
    else
      firecracker_bin="/usr/local/bin/$requested_firecracker_bin"
    fi
    ;;
esac

if command -v apt-get >/dev/null 2>&1; then
  sudo apt-get update -qq
  sudo apt-get install -y -qq ca-certificates curl iproute2 iptables kmod tar
elif command -v dnf >/dev/null 2>&1; then
  sudo dnf install -y ca-certificates curl iproute iptables kmod tar
elif command -v yum >/dev/null 2>&1; then
  sudo yum install -y ca-certificates curl iproute iptables kmod tar
else
  echo "unsupported package manager; install ca-certificates curl iproute2/iproute iptables kmod tar manually" >&2
  exit 1
fi

sudo modprobe kvm 2>/dev/null || true
sudo modprobe kvm_intel 2>/dev/null || sudo modprobe kvm_amd 2>/dev/null || true

if [ -x "$firecracker_bin" ]; then
  printf '%s\n' "firecracker already installed at $firecracker_bin"
else

  arch="$(uname -m)"
  case "$arch" in
    x86_64|amd64) asset_arch="x86_64" ;;
    aarch64|arm64) asset_arch="aarch64" ;;
    *) echo "unsupported Firecracker architecture: $arch" >&2; exit 1 ;;
  esac

  release_json="$(curl -fsSL https://api.github.com/repos/firecracker-microvm/firecracker/releases/latest)"
  asset_url="$(printf '%s\n' "$release_json" \
    | sed -n 's/.*"browser_download_url": "\([^"]*firecracker-[^"]*-'"$asset_arch"'\.tgz\)".*/\1/p' \
    | head -n 1)"

  if [ -z "$asset_url" ]; then
    echo "could not find Firecracker release asset for $asset_arch" >&2
    exit 1
  fi

  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  curl -fsSL "$asset_url" -o "$tmp/firecracker.tgz"
  tar -xzf "$tmp/firecracker.tgz" -C "$tmp"
  firecracker_src="$(find "$tmp" -type f -name 'firecracker-*' -perm -111 | head -n 1)"

  if [ -z "$firecracker_src" ]; then
    echo "Firecracker binary not found in release archive" >&2
    exit 1
  fi

  sudo mkdir -p "$(dirname "$firecracker_bin")"
  sudo install -m 0755 "$firecracker_src" "$firecracker_bin"
  printf '%s\n' "firecracker installed at $firecracker_bin"
fi

if [ -n "$caddy_config_dir" ]; then
  if command -v caddy >/dev/null 2>&1; then
    printf '%s\n' "caddy already installed"
  else
    if command -v apt-get >/dev/null 2>&1; then
      sudo apt-get install -y -qq debian-keyring debian-archive-keyring apt-transport-https gnupg
      curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
        | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
      curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
        | sudo tee /etc/apt/sources.list.d/caddy-stable.list
      sudo apt-get update -qq
      sudo apt-get install -y -qq caddy
    else
      echo "Caddy auto-install currently requires apt-get; install caddy manually for this distribution" >&2
      exit 1
    fi
  fi

  sudo mkdir -p "$caddy_config_dir"
  sudo systemctl enable caddy
  sudo systemctl start caddy
  printf '%s\n' "caddy ready with config dir $caddy_config_dir"
fi
