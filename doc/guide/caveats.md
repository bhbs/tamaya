# Caveats

Tamaya is intentionally narrow. It is designed to run self-contained applications on a single Linux VPS with less deployment machinery than a container-based platform.

That tradeoff keeps the system small, but it also defines where Tamaya fits.

## Single-Server Platform

Tamaya runs applications on a single Linux worker. It does not currently provide multi-node scheduling, automatic failover, or horizontal autoscaling.

If the VPS goes down, every application on that worker goes down with it. Backups, monitoring, and server recovery remain the operator's responsibility.

## Worker Requirements

The worker must be a Linux server with systemd and cgroup v2 (the default on modern distributions). `tamaya setup` also expects:

- An `x86_64` or `aarch64` worker
- SSH access
- `sudo`
- `apt-get`, `dnf`, or `yum`
- Outbound network access for downloading Caddy

The following tools are required on the worker and are checked by `tamaya check`:

- `systemctl` (systemd)
- `ss`, `flock`, `curl`, `tar`
- Caddy installed and running

## Application Packaging

Deploys accept a single Linux executable. The binary must match the worker architecture and must include, or explicitly arrange for, its runtime dependencies.

Native static binaries are the simplest fit. Tamaya is not a replacement for a container image when an application needs a full userspace, multiple processes, or arbitrary operating-system packages.

## Isolation Boundaries

Each application runs as a dedicated system user under a hardened systemd service. Sandboxing includes `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`, `PrivateTmp`, and configurable resource limits.

This is a meaningful boundary, but not a complete security guarantee. Applications still share the worker's host kernel, filesystem, and Caddy entry point. Tamaya does not replace host patching, access control, monitoring, or backups.

Published apps are different: Caddy serves those files directly from the worker filesystem, without a sandboxed application process. Published artifacts should come from a trusted build process.

## Deploy Availability

Applications use health-checked blue-green deploys. Zero downtime depends on the application starting successfully, passing its health check, and handling the traffic switch correctly.

Health checks only prove that the configured HTTP endpoint returned success. They do not prove that background workers, migrations, external services, or every application route is healthy.

Persistent data in the `data/` directory is accessible to both the old and new release during deployment. Applications that perform write-heavy workloads should handle concurrent access and schema changes gracefully.

Published static and SPA apps are route-switched, not process blue-green deployed. Tamaya uploads files into a release-specific directory and updates Caddy to point at that directory, but there is no application process or health check. Rollback switches the route back to the previous published release.

## Persistent Storage

Tamaya keeps SQLite databases, uploads, and other application data in a persistent `data/` directory across deploys.

Persistence is not backup, replication, or database migration management. Operators should back up application data and test recovery procedures.

Tamaya does not coordinate database migrations between old and new releases. During a blue-green deploy, both versions may access the same persistent data directory for a short time.

## Secrets and Worker Access

Environment values are stored on the worker in root-owned plaintext files and are injected by systemd when a release starts. Tamaya avoids printing stored values during normal CLI operations, but host root, backups, and a compromised deployment user can access them.

The SSH deployment user needs root or passwordless sudo access for setup, systemd units, environment files, service control, and Caddy updates. Treat compromise of that SSH key as a potential worker compromise.
