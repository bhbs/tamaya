# Directory Layout

## Local Controller

```text
./.tamaya.toml
```

Project-local settings live in `.tamaya.toml`, discovered upward from the current directory. Project settings can define the app name, deploy or publish inputs, and the selected worker SSH alias.

## Worker — Process App

```text
/var/lib/tamaya/apps/<app>/
├── releases/<release>/app
├── current
├── previous
├── data/                 # persistent app data; also the tamaya-<app> user's $HOME
├── metadata.toml
└── deploy.lock

/etc/tamaya/apps/<app>.env
/etc/systemd/system/tamaya-<app>-<release>.service
/etc/caddy/conf.d/<app>.caddy
```

## Worker — Published App

```text
/var/lib/tamaya/apps/<app>/
├── releases/<release>/site/
├── current
├── previous
├── metadata.toml
└── deploy.lock

/etc/caddy/conf.d/<app>.caddy
```

Published site files are owned by `root:root`. Published apps have no systemd unit or environment file.

Environment values are stored only on the worker. Worker environment files use mode `0600` and are owned by `root:root`; systemd reads them before starting the unprivileged `tamaya-<app>` application process.

The `tamaya-<app>` user's passwd home directory is `data/`. At runtime `$HOME` matches `TAMAYA_DATA_DIR`. See [Service user home](../guide/architecture.md#service-user-home).
