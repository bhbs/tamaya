set -u
progress "checking worker readiness"
data_dir={{data}}
fail=0

if test "$(uname -s)" = Linux; then
  echo "PASS Linux"
else
  echo "FAIL Linux (kernel is $(uname -s), expected Linux)"
  fail=1
fi

if command -v systemctl >/dev/null 2>&1; then
  echo "PASS systemctl"
else
  echo "FAIL systemctl (command not found)"
  fail=1
fi

if test -d /sys/fs/cgroup; then
  if test -f /sys/fs/cgroup/cgroup.controllers; then
    echo "PASS cgroup-v2"
  else
    echo "FAIL cgroup-v2 (cgroup.controllers not found; cgroup v1 is not supported)"
    fail=1
  fi
else
  echo "FAIL cgroup-v2 (/sys/fs/cgroup not mounted)"
  fail=1
fi

for cmd in ss flock curl tar; do
  if command -v "$cmd" >/dev/null 2>&1; then
    echo "PASS $cmd"
  else
    echo "FAIL $cmd (command not found)"
    fail=1
  fi
done

if command -v caddy >/dev/null 2>&1; then
  echo "PASS caddy"
  if systemctl is-active --quiet caddy 2>/dev/null; then
    echo "PASS caddy-active"
  else
    echo "FAIL caddy-active (caddy is installed but not running)"
    fail=1
  fi
else
  echo "FAIL caddy (command not found)"
  echo "SKIP caddy-active"
  fail=1
fi

if sudo mkdir -p "$data_dir/apps" 2>/dev/null; then
  echo "PASS data-dir"
else
  echo "FAIL data-dir (cannot create $data_dir/apps)"
  fail=1
fi

progress "worker readiness check complete"
if test "$fail" -ne 0; then
  exit 1
fi
