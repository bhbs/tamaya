# `.tamaya.toml` Reference

Project configuration lives in `.tamaya.toml` and is discovered upward from the current directory. Command-line flags override project settings. Unknown keys are rejected.

## Shared Fields

```toml
name = "api"                 # optional when app is passed on CLI
worker = "tamaya-prod"       # optional OpenSSH host alias
domain = "api.example.com"   # optional for deploy, required for publish
path = "/api"                # optional; requires domain
```

`worker` is passed directly to `ssh`; connection details belong in `~/.ssh/config`.

Set `path` to route under a URL prefix. Without `path`, the app owns the domain root. `path = "/"` should be treated as root routing and usually omitted.

Prefix `domain` with `http://` only when TLS terminates at an upstream proxy and Caddy should not obtain a certificate. `https://` domains are not accepted.

## Process App Deploys

Process apps use `tamaya deploy` and must set `binary`.

```toml
name = "api"
worker = "tamaya-prod"
binary = "./dist/api"
domain = "api.example.com"

[health_check]
path = "/health"
retries = 5
interval_secs = 5
timeout_secs = 2
```

Fields:

- `binary`: local executable path to upload.
- `writable_release`: default `false`; set `true` only for self-extracting binaries such as Bun compile artifacts that write beside themselves.
- `verify_binary_deps`: default `false`; set `true` to run `ldd` on the worker after upload and fail before systemd starts when shared libraries are missing.
- `[health_check]`: optional; defaults are shown above.

The executable must bind to the `PORT` environment variable. Persistent app data is exposed as `TAMAYA_DATA_DIR`, and the service user's home also points to the persistent data directory.

## Published Static Sites and SPAs

Published apps use `tamaya publish`, must set `static_root`, and require `domain`.

```toml
name = "docs"
worker = "tamaya-prod"
domain = "docs.example.com"
static_root = "./dist"
publish_type = "spa"
```

Fields:

- `static_root`: local directory whose contents are uploaded.
- `publish_type`: `"static"` or `"spa"`; default is `"static"`.

Use `publish_type = "spa"` when missing paths should fall back to `index.html` for client-side routing. Use `"static"` or omit `publish_type` for ordinary static files that should return 404 for missing paths.

For path-scoped publishes, the path prefix must exist inside `static_root`. For example, `path = "/docs"` with `static_root = "./dist"` serves from `dist/docs/`; SPA mode requires `dist/docs/index.html`.

## Resource Limits

Add limits only when requested or clearly needed.

```toml
[memory]
max = "512M"

[cpu]
quota = "50%"
```

These values are passed through to systemd resource control. `memory.max` becomes `MemoryMax=`, so use systemd memory sizes such as `"512M"` or `"1G"`; `cpu.quota` becomes `CPUQuota=`, so use percentages such as `"50%"` or `"200%"`.

## Defaults

Prefer omitting fields whose defaults are acceptable.

| Key | Default when omitted |
|-----|----------------------|
| `name` | None; app name must come from the CLI command |
| `worker` | None; worker must come from the CLI command where supported |
| `binary` | None; required for `tamaya deploy` unless passed with `--binary` |
| `static_root` | None; required for `tamaya publish` unless passed with `--static-root` |
| `domain` | None for `deploy`; required for `publish` |
| `path` | None; root route when `domain` is set |
| `publish_type` | `"static"` |
| `writable_release` | `false` |
| `verify_binary_deps` | `false` |
| `[health_check].path` | `"/health"` |
| `[health_check].retries` | `5` |
| `[health_check].interval_secs` | `5` |
| `[health_check].timeout_secs` | `2` |
| `[memory].max` | None; no `MemoryMax=` is added |
| `[cpu].quota` | None; no `CPUQuota=` is added |

## Valid Top-Level Keys

- `name`
- `worker`
- `binary`
- `domain`
- `path`
- `static_root`
- `publish_type`
- `writable_release`
- `verify_binary_deps`
- `health_check`
- `memory`
- `cpu`

`binary` and `static_root` are mutually exclusive.
