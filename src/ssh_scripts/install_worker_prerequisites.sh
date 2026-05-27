set -eu
firecracker_bin={{firecracker_bin}}
caddy_config_dir={{caddy_config_dir}}

install_packages() {
  if command -v apt-get >/dev/null 2>&1; then
    sudo apt-get update -qq
    sudo apt-get install -y -qq "$@"
  elif command -v dnf >/dev/null 2>&1; then
    packages=""
    for package_name in "$@"; do
      case "$package_name" in
        iproute2) package_name="iproute" ;;
      esac
      packages="${packages:+$packages }$package_name"
    done
    sudo dnf install -y $packages
  elif command -v yum >/dev/null 2>&1; then
    packages=""
    for package_name in "$@"; do
      case "$package_name" in
        iproute2) package_name="iproute" ;;
      esac
      packages="${packages:+$packages }$package_name"
    done
    sudo yum install -y $packages
  else
    echo "unsupported package manager; install missing packages manually: $*" >&2
    exit 1
  fi
}

missing_base_packages=""
for command_name in curl ip iptables modprobe tar; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    case "$command_name" in
      ip) package_name="iproute2" ;;
      modprobe) package_name="kmod" ;;
      *) package_name="$command_name" ;;
    esac
    missing_base_packages="${missing_base_packages:+$missing_base_packages }$package_name"
  fi
done

if ! command -v update-ca-certificates >/dev/null 2>&1 && [ ! -d /etc/pki/ca-trust ]; then
  missing_base_packages="${missing_base_packages:+$missing_base_packages }ca-certificates"
fi

if [ -n "$missing_base_packages" ]; then
  install_packages $missing_base_packages
fi

case "$firecracker_bin" in
  */*)
    firecracker_installed=false
    [ -x "$firecracker_bin" ] && firecracker_installed=true
    ;;
  *)
    if command -v "$firecracker_bin" >/dev/null 2>&1; then
      firecracker_bin="$(command -v "$firecracker_bin")"
      firecracker_installed=true
    else
      firecracker_bin="/usr/local/bin/$firecracker_bin"
      firecracker_installed=false
    fi
    ;;
esac

sudo modprobe kvm 2>/dev/null || true
sudo modprobe kvm_intel 2>/dev/null || sudo modprobe kvm_amd 2>/dev/null || true

if [ "$firecracker_installed" = true ]; then
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

if command -v caddy >/dev/null 2>&1; then
  printf '%s\n' "caddy already installed"
else
  if command -v apt-get >/dev/null 2>&1; then
    install_packages debian-keyring debian-archive-keyring apt-transport-https gnupg
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
      | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
      | sudo tee /etc/apt/sources.list.d/caddy-stable.list
    install_packages caddy
  elif command -v dnf >/dev/null 2>&1 || command -v yum >/dev/null 2>&1; then
    install_packages caddy
  else
    echo "unsupported package manager; install caddy manually" >&2
    exit 1
  fi
fi

sudo mkdir -p "$caddy_config_dir"
sudo systemctl enable caddy
sudo systemctl start caddy
printf '%s\n' "caddy ready with config dir $caddy_config_dir"
