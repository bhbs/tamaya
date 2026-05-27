set -eu
[ "$(uname -s)" = "Linux" ]
[ -e /dev/kvm ]
[ -r /dev/kvm ]
[ -w /dev/kvm ]
if [ -n "$(id -u)" ]; then :; fi
command -v sh >/dev/null
command -v ip >/dev/null
command -v curl >/dev/null
if [ -x {{firecracker_bin}} ]; then
  :
else
  command -v {{firecracker_bin}} >/dev/null
fi
