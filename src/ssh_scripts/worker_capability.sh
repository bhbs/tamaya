set -eu
[ "$(uname -s)" = "Linux" ]
[ -e /dev/kvm ]
[ -r /dev/kvm ]
[ -w /dev/kvm ]
if [ -n "$(id -u)" ]; then :; fi
command -v sh >/dev/null
command -v ip >/dev/null
command -v curl >/dev/null
command -v tar >/dev/null
command -v truncate >/dev/null
command -v mkfs.ext4 >/dev/null || command -v mke2fs >/dev/null
if [ -x {{firecracker_bin}} ]; then
  :
else
  command -v {{firecracker_bin}} >/dev/null
fi
