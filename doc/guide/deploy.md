# Deploy

```bash
tamaya deploy [APP]
```

Tamaya uploads the configured binary, optionally checks shared library dependencies with `ldd` on the worker, allocates a port, passes it to the application as `PORT` together with `HOSTNAME=127.0.0.1`, starts a release-specific systemd unit, checks the health endpoint, switches Caddy traffic when a domain is configured, and stops the previous unit if it was running.

`APP` is optional when `name` is set in `.tamaya.toml`; otherwise pass the app name on the command line.

When both `domain` and `path` are omitted from the command line and project
configuration, redeploying preserves the app's existing domain and path from
worker metadata. A new app with no domain has no public route. An explicit
domain uses the configured path, or `/` when no path is specified. Tamaya checks
the resulting route for conflicts before uploading or starting the new release.

Enable the dependency check with `verify_binary_deps = true` in `.tamaya.toml` or `--verify-binary-deps` on the command line. When enabled and `ldd` is available on the worker, Tamaya runs it on the uploaded binary before committing the release or starting systemd. A recognized missing `.so` aborts the deploy and prints an install hint, for example `sudo dnf install -y libatomic` on RHEL-family workers or `sudo apt-get install -y libatomic1` on Debian/Ubuntu. If `ldd` is unavailable, Tamaya skips this check; it is a convenience check, not a complete ABI or compatibility guarantee.

The executable must bind to the `PORT` environment variable. Its working directory is the release-specific directory identified by its systemd unit; `current` is an operator-facing symlink and metadata pointer. The persistent data path is exposed as `TAMAYA_DATA_DIR`. The service user's `$HOME` is also `data/`; home-relative paths resolve there. See [Service user home](architecture.md#service-user-home).

Treat `PORT`, `HOSTNAME`, and `TAMAYA_DATA_DIR` as reserved names. Tamaya does not currently reject these keys in `tamaya env`, and the worker environment file is loaded after the generated values in the systemd unit, so duplicate keys override them.

By default, Tamaya checks `GET /health` with 5 retries, 5 seconds between attempts, and a 2 second timeout. Override this with `[health_check]` in `.tamaya.toml`.

If the new release fails its health check or Caddy cannot be updated, Tamaya cleans up the staged release and leaves the previous unit and route in their prior state.

Use `tamaya deploy --dry-run` to inspect the resolved local configuration without connecting to or changing the worker.
If no domain is configured locally, dry-run reports that the existing route will
be kept; it cannot show that route without reading worker metadata.

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
