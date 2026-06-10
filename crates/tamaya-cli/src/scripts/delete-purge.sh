test -n "$data_dir" && test -n "$app" && test "$app_dir" = "$data_dir/apps/$app" ||
  { echo "refusing to purge unexpected app directory: $app_dir" >&2; exit 1; }
case "$app_dir" in
  "$data_dir"/apps/*) ;;
  *) echo "refusing to purge unexpected app directory: $app_dir" >&2; exit 1 ;;
esac
sudo rm -rf "$app_dir"
sudo userdel "tamaya-$app" >/dev/null 2>&1 || true
