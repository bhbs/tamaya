sudo find "$app_dir/releases/$release" -mindepth 1 ! -path "$app_dir/releases/$release/app" \
  -exec chown "tamaya-$app":"tamaya-$app" {} +
sudo chown root:"tamaya-$app" "$app_dir/releases/$release"
sudo chmod 1775 "$app_dir/releases/$release"
sudo chown root:root "$app_dir/releases/$release/app"
sudo chmod 0755 "$app_dir/releases/$release/app"
