# Quick Start

Add a worker alias to `~/.ssh/config`:

```sshconfig
Host tamaya-prod
  HostName your.server.ip
  User deploy
  IdentityFile ~/.ssh/id_ed25519
```

Run setup once, then verify the worker:

```bash
tamaya setup --worker tamaya-prod
tamaya check --worker tamaya-prod
```

Configure an app in your repository's `.tamaya.toml`:

```toml
name = "api"
worker = "tamaya-prod"
binary = "./dist/api"
domain = "api.example.com"
```

Tamaya passes the worker value directly to `ssh`, so connection settings stay
in `~/.ssh/config`.

The binary must be built for the worker's Linux architecture, listen on `PORT`, and return success from `GET /health`.

Use `domain = "http://api.example.com"` when TLS terminates at an upstream proxy and Caddy should serve the origin over HTTP only.

Set application secrets before deploying:

```bash
tamaya env api set DATABASE_URL
```

See [environment variables and secrets](environment.md) for storage details and worker privilege requirements.

Deploy and inspect the app:

```bash
tamaya deploy
tamaya status
tamaya logs api
```

Roll back to the previous release if needed:

```bash
tamaya rollback api
```

For static sites and SPAs, use `static_root` and `tamaya publish`; see [publishing](publish.md).
