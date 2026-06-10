{{prelude}}
app_type="$(value app_type)"
test -n "$app_type" || app_type="process"
test "$app_type" != "published" || { echo "$app is a published app and does not have systemd logs" >&2; exit 1; }
unit="$(value unit)"
progress "streaming logs"
exec 6>&-
exec journalctl -u "$unit" -f
