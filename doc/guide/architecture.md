# Architecture

The local `tamaya` CLI sends shell programs and binary streams to a Linux worker over SSH. Worker-side metadata is the source of truth, so multiple controller machines observe the same deployment state.

```text
local CLI -> SSH -> worker metadata, systemd, Caddy
Caddy -> 127.0.0.1:<allocated-port> -> systemd release unit
```

Each process app receives a dedicated `tamaya-<app>` Linux user. Both process and published apps receive a directory under `/var/lib/tamaya/apps/<app>/`. Process app directories include:

```text
/var/lib/tamaya/apps/<app>/
├── releases/
├── current
├── previous
├── data/
├── metadata.toml
└── deploy.lock
```

Deploys use an app lock and a worker-wide port allocation lock. Failed releases are stopped and removed before traffic changes.

## Linux User Lifecycle

The `tamaya-<app>` system user is created on first deploy:

```
sudo useradd --system --home /var/lib/tamaya/apps/<app>/data --shell /usr/sbin/nologin tamaya-<app>
```

- `--system` — low UID, no login
- `--home` — passwd home directory is the persistent `data/` directory (see [Service user home](#service-user-home))
- `--shell /usr/sbin/nologin` — no interactive shell

The user is referenced by the systemd unit (`User=tamaya-<app>`) and owns the `data/` directory. For self-extracting binaries (`writable_release = true`) the release directory is also chowned to this user.

### Service user home

Tamaya maps the service user's home directory to `data/`. The systemd unit does not set `HOME` explicitly; libc and language runtimes resolve it from the user database.

At runtime for process apps:

- `$HOME` is `/var/lib/tamaya/apps/<app>/data`, the same path as `TAMAYA_DATA_DIR`
- Tilde expansion and home-directory APIs (for example Node `os.homedir()` or Python `Path.home()`) resolve to `data/`
- Default home-relative paths such as `~/.cache/<name>` are created under `data/.cache/<name>`

`ProtectHome=yes` hides `/home`, `/root`, and `/run/user`. It does not block `data/` because that path lives under `/var/lib/tamaya`, not `/home`.

Libraries and frameworks that write cache or configuration to the user's home directory therefore land in the persistent `data/` tree unless the application reads `TAMAYA_DATA_DIR` or a custom environment variable overrides the location.

Published (static/SPA) apps do not create a user because Caddy reads the site files as root.

The user is removed only by `tamaya delete --purge`. A plain `tamaya delete` keeps the user and the `data/` directory.

## Data Directory

The `data/` directory is the application's persistent workspace. It survives deploys, rollbacks, and non-purge deletes.

### Lifetime

```
tamaya deploy (first) → creates data/, chowns to tamaya-<app>
tamaya deploy (again) → data/ untouched; old and new releases see the same data
tamaya rollback        → data/ untouched
tamaya delete          → only data/ kept under apps/<app>/
tamaya delete --purge  → data/ deleted with the rest of the app
```

### Process Environment

The systemd unit enforces a strict sandbox and wires the directory into the application process:

```
WorkingDirectory=/var/lib/tamaya/apps/<app>/releases/<release>
Environment=PORT=<allocated-port>
Environment=HOSTNAME=127.0.0.1
Environment=TAMAYA_DATA_DIR=/var/lib/tamaya/apps/<app>/data
EnvironmentFile=-/etc/tamaya/apps/<app>.env
ReadWritePaths=/var/lib/tamaya/apps/<app>/data
```

The systemd unit sets the process working directory to the active **release** directory (the same tree as `ExecStart`). After a successful deploy, `current/` is a symlink to that release for operators and metadata; systemd does not depend on it at start time. The persistent data path is available as the `TAMAYA_DATA_DIR` environment variable. The allocated localhost port is available as `PORT`, and worker-stored environment variables are loaded from `/etc/tamaya/apps/<app>.env`.

Outside `data/`, the service is constrained by the systemd sandbox: host paths are read-only, hidden, or isolated except where systemd provides private runtime locations such as `/tmp`.

When `writable_release = true`, the release directory is added to `ReadWritePaths` so self-extracting binaries can write runtime assets beside the executable. The working directory is that same release directory. The binary itself is kept owned by root with mode `0755` so the application process cannot overwrite it.

By default (`writable_release = false`) the binary and its release directory are read-only. The process can only write to `data/`.

### Sandbox

| systemd directive | Effect |
|---|---|
| `NoNewPrivileges=yes` | Process cannot gain privileges via setuid or capabilities |
| `ProtectSystem=strict` | `/usr`, `/boot`, `/etc` are read-only; `/dev` is minimal |
| `ProtectHome=yes` | `/home`, `/root`, `/run/user` are invisible |
| `PrivateTmp=yes` | Private `/tmp` and `/var/tmp`, invisible to other processes |
| `ReadWritePaths=/var/lib/tamaya/apps/<app>/data` | Only writable path by default. The binary and its release directory are read-only. `writable_release = true` adds the release directory |

### Published Apps

Published (static/SPA) apps have no `data/` directory and no sandboxed process. Caddy serves site files directly from the release directory as root.
