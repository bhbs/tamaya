report_health_check_failure() {
  unit="$1"
  target="$2"
  echo "health check failed for $target" >&2
  echo "--- systemctl status $unit ---" >&2
  sudo systemctl status "$unit" --no-pager -l >&2 || true
  echo "--- journalctl -u $unit (last 40 lines) ---" >&2
  sudo journalctl -u "$unit" -n 40 --no-pager >&2 || true
}
