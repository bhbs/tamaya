# `.tamaya.toml`

```toml
name = "blog"
binary = "./dist/blog"
domain = "blog.example.com" # optional
path = "/api"               # optional; requires domain
worker = "tamaya-prod"      # optional; OpenSSH host alias
writable_release = false    # optional; allow self-extracting binaries to write beside themselves
verify_binary_deps = false  # optional; run ldd on the worker after upload

[health_check]
path = "/health"
retries = 5
interval_secs = 5
timeout_secs = 2

[memory]
max = "512M"

[cpu]
quota = "50%"
```

Published apps use `static_root` instead of `binary`:

```toml
name = "docs"
domain = "docs.example.com"
path = "/docs"              # optional; publish requires domain
static_root = "./dist"
publish_type = "spa"        # optional; "static" (default) or "spa"
```

Prefix `domain` with `http://` when TLS terminates at an upstream proxy and Caddy should serve the origin over HTTP without obtaining a certificate.

Set `path` to deploy or publish under a URL prefix on the configured domain. Without `path`, the app owns the domain root. Path routes require `domain`.

Releases are immutable by default. Set `writable_release = true` only for self-extracting binaries such as Bun Compile artifacts that must write runtime assets beside the executable.

`[memory].max` and `[cpu].quota` are systemd resource-control values. Tamaya writes them to the service unit as `MemoryMax=` and `CPUQuota=`. For example, `max = "512M"` becomes `MemoryMax=512M`, and `quota = "50%"` becomes `CPUQuota=50%`.

## Defaults

| Key | Default when omitted |
|-----|----------------------|
| `name` | None; pass the app name on the command line |
| `worker` | None; pass `--worker` for commands that accept it |
| `binary` | None; required for `tamaya deploy` unless passed with `--binary` |
| `static_root` | None; required for `tamaya publish` unless passed with `--static-root` |
| `domain` | None for `deploy`; required for `publish` |
| `path` | None; the app owns the domain root when `domain` is set |
| `publish_type` | `"static"` |
| `writable_release` | `false` |
| `verify_binary_deps` | `false` |
| `[health_check].path` | `"/health"` |
| `[health_check].retries` | `5` |
| `[health_check].interval_secs` | `5` |
| `[health_check].timeout_secs` | `2` |
| `[memory].max` | None; no `MemoryMax=` is added |
| `[cpu].quota` | None; no `CPUQuota=` is added |

## `deploy` or `publish`

Process apps use `binary` and are deployed with `tamaya deploy`. Published apps use `static_root` and `publish_type` and are deployed with `tamaya publish`. The two modes are mutually exclusive: a `.tamaya.toml` must define either `binary` or `static_root`, not both.

`publish_type` accepts `"static"` (default) or `"spa"`. SPA mode serves `index.html` for missing files to support client-side routing. Static mode returns 404 for missing files.

`worker` selects an OpenSSH host alias. Tamaya passes the value directly to
`ssh`, so define connection details in `~/.ssh/config`:

```sshconfig
Host tamaya-prod
  HostName 203.0.113.10
  User deploy
  Port 22
  IdentityFile ~/.ssh/id_ed25519
```

Unknown keys are rejected.
