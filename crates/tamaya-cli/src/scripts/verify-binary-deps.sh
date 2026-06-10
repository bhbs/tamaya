progress "checking binary dependencies"
if ! command -v ldd >/dev/null 2>&1; then
  exit 0
fi
ldd_output="$(ldd "$binary" 2>&1)" || ldd_output=""
if printf '%s' "$ldd_output" | grep -q 'not a dynamic executable'; then
  exit 0
fi
missing_libs="$(printf '%s\n' "$ldd_output" | sed -n 's/^[[:space:]]*\([^[:space:]]*\) => not found$/\1/p')"
if test -z "$missing_libs"; then
  exit 0
fi

pkg_family=unknown
if test -r /etc/os-release; then
  # shellcheck disable=SC1091
  . /etc/os-release
  case "${ID:-}:${ID_LIKE:-}" in
    *debian*|*ubuntu*) pkg_family=debian ;;
    *rhel*|*centos*|*fedora*|*rocky*|*almalinux*) pkg_family=rhel ;;
  esac
fi

package_for_lib() {
  lib="$1"
  if test "$pkg_family" = debian; then
    case "$lib" in
      libatomic.so.1) printf '%s' libatomic1 ;;
      libstdc++.so.6) printf '%s' libstdc++6 ;;
      libgcc_s.so.1) printf '%s' libgcc-s1 ;;
      libz.so.1) printf '%s' zlib1g ;;
      libssl.so.3) printf '%s' libssl3 ;;
      libcrypto.so.3) printf '%s' libssl3 ;;
      libsqlite3.so.0) printf '%s' libsqlite3-0 ;;
    esac
  elif test "$pkg_family" = rhel; then
    case "$lib" in
      libatomic.so.1) printf '%s' libatomic ;;
      libstdc++.so.6) printf '%s' libstdc++ ;;
      libgcc_s.so.1) printf '%s' libgcc ;;
      libz.so.1) printf '%s' zlib ;;
      libssl.so.3) printf '%s' openssl-libs ;;
      libcrypto.so.3) printf '%s' openssl-libs ;;
      libsqlite3.so.0) printf '%s' sqlite-libs ;;
    esac
  fi
}

install_cmd() {
  pkgs="$1"
  if test "$pkg_family" = debian; then
    printf 'sudo apt-get install -y %s' "$pkgs"
  elif test "$pkg_family" = rhel; then
    if command -v dnf >/dev/null 2>&1; then
      printf 'sudo dnf install -y %s' "$pkgs"
    else
      printf 'sudo yum install -y %s' "$pkgs"
    fi
  else
    printf 'install OS package providing %s' "$pkgs"
  fi
}

provides_hint() {
  lib="$1"
  if test "$pkg_family" = debian; then
    printf "sudo apt-file search %s  # or: dpkg -S %s" "$lib" "$lib"
  elif test "$pkg_family" = rhel; then
    printf "sudo dnf provides '*/%s'" "$lib"
  else
    printf "use your package manager to install a package providing %s" "$lib"
  fi
}

echo "binary is missing shared libraries on the worker:" >&2
while IFS= read -r lib; do
  test -n "$lib" || continue
  pkg="$(package_for_lib "$lib")"
  if test -n "$pkg"; then
    echo "  $lib — run: $(install_cmd "$pkg")" >&2
  else
    echo "  $lib — find package: $(provides_hint "$lib")" >&2
  fi
done <<EOF
$missing_libs
EOF
echo "deploy aborted: install missing libraries on the worker and retry" >&2
exit 1
