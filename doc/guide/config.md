# Configuration

Project configuration lives in `.tamaya.toml` and is discovered upward from the current directory.

Configuration is resolved from most specific to least specific:

1. Command-line options, such as `--worker`, `--binary`, or `--domain`.
2. Project settings in `.tamaya.toml`.

## Worker Config

Configure SSH connection settings in `~/.ssh/config`:

```sshconfig
Host tamaya-prod
  HostName 203.0.113.10
  User deploy
  Port 22
  IdentityFile ~/.ssh/id_ed25519
```

Select the worker in `.tamaya.toml`:

```toml
worker = "tamaya-prod"
```

The `worker` value is an OpenSSH host alias and is passed directly to `ssh`.
Use `--worker <alias>` to override it for commands that accept a worker option.

## Process apps

Process apps use `binary` and are deployed with `tamaya deploy`:

```toml
name = "blog"
binary = "./dist/blog"
domain = "blog.example.com"
worker = "tamaya-prod"
writable_release = false
verify_binary_deps = false

[health_check]
path = "/health"
retries = 5
interval_secs = 5
timeout_secs = 2
```

`path` is optional. Without it, the app owns the domain root. Set a prefix such as `path = "/api"` to route only that path on the configured domain. Path routes require `domain`.

The `[health_check]` table is optional; the values above are the defaults.

Set `writable_release = true` only for self-extracting binaries that must write runtime assets beside the executable. Releases are immutable by default.

Set `verify_binary_deps = true` to run `ldd` on the worker after upload and abort the deploy before systemd starts if shared libraries are missing.

Resource limits are passed through to systemd. `[memory].max` is written as `MemoryMax=` and accepts systemd memory sizes such as `"512M"` or `"1G"`. `[cpu].quota` is written as `CPUQuota=` and accepts percentages such as `"50%"` or `"200%"`.

Omitted resource limits are not added to the generated systemd unit.

## Published apps

Published apps use `static_root` instead of `binary` and are deployed with `tamaya publish`:

```toml
name = "docs"
domain = "docs.example.com"
path = "/docs"
static_root = "./dist"
publish_type = "spa"
```

`publish_type` accepts `"static"` or `"spa"`. Static is the default. SPA mode serves `index.html` for missing files so client-side routes work.

See the [configuration reference](../reference/tamaya-toml.md#defaults) for all default values.

## Domains

Use an `http://` prefix, such as `domain = "http://blog.example.com"`, when TLS terminates at an upstream proxy and Caddy should not obtain a certificate.
Without the prefix, Caddy serves HTTPS and obtains certificates automatically.
`https://` domains are not accepted.

Unknown keys are rejected so configuration typos fail fast.

See the [configuration reference](../reference/tamaya-toml.md).
