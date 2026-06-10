# Publish

```bash
tamaya publish [app]
```

Tamaya archives a local directory, uploads the tarball to the worker, extracts site files into a release-specific directory, and updates the Caddy route.

Publish is designed for pre-built static assets: HTML, CSS, JavaScript, images, and other files that Caddy can serve directly without a process.

## Configuration

```toml
name = "docs"
domain = "docs.example.com"
static_root = "./dist"
publish_type = "spa"
```

`domain` is required for published apps. `static_root` points to the local directory whose contents should be uploaded; it must exist and must not be empty. `publish_type` accepts `"static"` (default) or `"spa"`.

## Static vs SPA

- **static**: Caddy serves files as-is and returns 404 for missing paths.
- **spa**: Caddy falls back to `index.html` for any missing path, enabling client-side routing.

Routes can be root (the entire domain) or path-scoped:

```toml
path = "/docs"
```

For path-scoped publishes, the path prefix must exist inside `static_root`. For example, `path = "/docs"` with `static_root = "./dist"` serves files from `dist/docs/`. In SPA mode, Tamaya requires `index.html` at the served root, so that example must include `dist/docs/index.html`.

## Examples

API apps are process apps, so deploy them with `tamaya deploy` and route them with `path`:

```toml
name = "api"
domain = "example.com"
path = "/api"
binary = "./dist/api"

[health_check]
path = "/health"
```

```bash
tamaya deploy
```

Static sites are published as files and return 404 for missing paths:

```toml
name = "docs"
domain = "example.com"
path = "/docs"
static_root = "./dist"
publish_type = "static"
```

```text
dist/
`-- docs/
    |-- index.html
    |-- guide.html
    `-- 404.html
```

```bash
tamaya publish
```

SPA sites are also published as files, but missing paths fall back to `index.html`:

```toml
name = "app"
domain = "example.com"
path = "/app"
static_root = "./dist"
publish_type = "spa"
```

```text
dist/
`-- app/
    |-- index.html
    `-- assets/
        `-- app.js
```

```bash
tamaya publish
```

## CLI Flags

Override `.tamaya.toml` values on the command line:

| Flag | Description |
|------|-------------|
| `--worker` | Select a specific worker by name |
| `--path` | Override the path prefix |
| `--publish-type` | Override `publish_type` (`static` or `spa`) |
| `--static-root` | Override the local directory to upload |

## How It Works

1. Tar the local `static_root` directory (skipping `.git` and `.env` entries).
2. Upload the tarball to the worker over SSH.
3. Extract into `/var/lib/tamaya/apps/<app>/releases/<release>/site/`.
4. Set ownership to `root:root` and permissions to `0644` (files) / `0755` (dirs).
5. Generate a Caddy route snippet referencing the site directory.
6. Rebuild the merged domain Caddyfile and reload Caddy.

Old releases are kept for 5 generations. `tamaya rollback` switches the Caddy route back to the previous release without allocating a port or running a health check.
