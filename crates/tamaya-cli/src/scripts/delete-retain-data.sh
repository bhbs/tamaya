test -n "$data_dir" && test -n "$app" && test "$app_dir" = "$data_dir/apps/$app" ||
  { echo "refusing to clean unexpected app directory: $app_dir" >&2; exit 1; }
case "$app_dir" in
  "$data_dir"/apps/*) ;;
  *) echo "refusing to clean unexpected app directory: $app_dir" >&2; exit 1 ;;
esac
sudo find "$app_dir" -mindepth 1 -maxdepth 1 ! -name data -exec rm -rf {} +
