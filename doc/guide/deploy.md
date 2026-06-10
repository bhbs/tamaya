# Deploy

```bash
tamaya deploy [APP]
```

Tamaya uploads the configured binary, optionally runs `ldd` on the worker to verify shared library dependencies, allocates a localhost port, starts a release-specific systemd unit, checks the health endpoint, switches Caddy traffic, and stops the previous unit.

`APP` is optional when `name` is set in `.tamaya.toml`; otherwise pass the app name on the command line.

Enable the dependency check with `verify_binary_deps = true` in `.tamaya.toml` or `--verify-binary-deps` on the command line. When enabled, Tamaya runs `ldd` on the uploaded binary before committing the release or starting systemd. A missing `.so` aborts the deploy and prints an install hint, for example `sudo dnf install -y libatomic` on RHEL-family workers or `sudo apt-get install -y libatomic1` on Debian/Ubuntu.

The executable must bind to the `PORT` environment variable. Its working directory is the active release directory (`current/`) and the persistent data path is exposed as `TAMAYA_DATA_DIR`. The service user's `$HOME` is also `data/`; home-relative paths resolve there. See [Service user home](architecture.md#service-user-home).

By default, Tamaya checks `GET /health` with 5 retries, 5 seconds between attempts, and a 2 second timeout. Override this with `[health_check]` in `.tamaya.toml`.

If the new release fails its health check or Caddy cannot be updated, Tamaya cleans up the staged release and leaves the previous release active.

Use `tamaya deploy --dry-run` to inspect the resolved local configuration without connecting to or changing the worker.

## CLI Flags

Override `.tamaya.toml` values on the command line:

| Flag | Description |
|------|-------------|
| `--worker` | Select a specific worker by name |
| `--binary` | Override the binary path |
| `--domain` | Override the domain |
| `--path` | Override the path prefix |
| `--dry-run` | Print resolved configuration without deploying |
| `--verify-binary-deps` | Run `ldd` on the worker before starting the release |
