# AGENTS.md

## Project Overview

Tamaya is a single-binary-first VPS deployment platform. A local CLI manages
remote Linux workers exclusively over SSH. Workers use systemd for process
supervision and Caddy for reverse proxying and TLS.

## Commands

```bash
just build
just test
just fmt
just clippy
just ci
just coverage
```

## Architecture

- `crates/tamaya-cli/src/app/`: command implementations.
- `crates/tamaya-cli/src/ssh.rs`: SSH transport and worker-side shell programs.
- `crates/tamaya-cli/src/config.rs`: global and per-project TOML configuration.
- `crates/tamaya-cli/src/env.rs`: remote app environment-variable management.

Deploys upload an executable into `/var/lib/tamaya/apps/<app>/releases/`,
allocate a localhost port, start a release-specific systemd unit, health-check
it, update Caddy, and stop the old unit. Worker-side metadata is the source of
truth. Persistent app data lives in `data/`.

## Scope

v1 intentionally excludes containers, Firecracker, KVM, managed databases, and build automation.
