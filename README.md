# Tamaya

Deploy apps, not containers.

Tamaya is a self-hosted deployment tool for single Linux executables. A local
CLI uploads a binary to a systemd VPS over SSH, starts a new release, checks its
health endpoint, switches Caddy traffic, and stops the previous release.

It is designed for small services that need inexpensive always-on hosting and
simple persistent storage such as SQLite.

## Features

- Single-binary deploys without Docker, Kubernetes, or KVM
- Health-checked blue-green releases with automatic rollback on failure
- Fast `tamaya rollback`
- Per-app Linux users and hardened systemd units
- Persistent `data/` directories for SQLite, uploads, and configuration
- Automatic localhost port allocation for concurrent releases
- Caddy reverse proxying and automatic HTTPS
- journalctl-backed logs and systemd-backed status

## Install

```bash
cargo install tamaya --locked
```

From this repository:

```bash
cargo install --path crates/tamaya-cli --locked
```

Requires Rust 1.95 or newer.

## Quick Start

See the [quick start guide](doc/guide/quickstart.md) for worker setup,
project configuration, and the first deploy.

See the [documentation](doc/guide/index.md) and
[configuration reference](doc/reference/tamaya-toml.md) for details.

## Development

```bash
just ci
just coverage
```
