---
lang: en-US
title: How Tamaya Turns a Linux Server into a Deployment Platform
description: How Tamaya combines systemd, cgroups, per-app users, Caddy, and SSH on a Linux worker into a consistent single-binary deployment lifecycle.
---

# How Tamaya Turns a Linux Server into a Deployment Platform

[Why Indie Developers Should Consider a Single VPS](./why-single-vps.md) explains how a single VPS can balance cost, performance, and operational clarity for workloads that fit on one machine. [How Far Can Linux Go as an Application Platform Without Containers?](./linux-application-platform.md) examines host-level mechanisms such as Linux users, systemd, cgroups, and namespaces.

So what does Tamaya add on top of them?

Tamaya does not invent a new process-isolation mechanism or a dedicated container runtime. Its local CLI operates a Linux worker over SSH and assembles capabilities already provided by Linux and Caddy into a repeatable, per-application deployment procedure.

```text
local repository
  └─ tamaya CLI
       └─ SSH
            └─ Linux worker
                 ├─ releases/ + metadata
                 ├─ systemd + cgroups
                 ├─ journal
                 ├─ Caddy
                 └─ data/
```

The important distinction is between what Linux can do, what Tamaya configures, and the state transitions Tamaya automates. The following table focuses on Tamaya's core model: running executables under systemd.

| Concern | Host-provided mechanism | What Tamaya automates |
| --- | --- | --- |
| Application privileges | Linux users and file permissions | Creates a dedicated, unprivileged user for each app and sets it in the service's `User=` directive |
| Process supervision | systemd | Generates a unit for each release, manages start and stop operations, and configures restart-on-failure behavior |
| Host write restrictions | Linux file permissions and systemd mount restrictions | Adds hardening directives and a dedicated persistent write location to the unit |
| CPU and memory control | cgroup v2 and systemd resource control | Converts configured limits into `CPUQuota=` and `MemoryMax=` |
| Logs | systemd journal | Identifies the current release and reads its journal through `tamaya logs` |
| HTTP(S) entry point | Caddy | Generates routes by domain and path, then switches the upstream after a health check succeeds |
| Administrative channel | OpenSSH and sudo | Uses a worker alias and runs standardized procedures from the CLI |
| Release history | Linux directories and symlinks | Defines release placement, metadata, `current` / `previous`, and rollback rules |

systemd, cgroups, and user permissions are Linux capabilities; Caddy and OpenSSH are not Tamaya-specific technologies either. Tamaya's value lies not merely in choosing these components, but in configuring them in the same order every time and combining their failure behavior into a single release lifecycle. This lifecycle does not narrow the permissions granted by SSH or sudo. Outside Tamaya, the same SSH user can still perform any action permitted by its sudo policy.

## Binary Deployment Is at the Core of Tamaya's Process Apps

Tamaya supports two delivery models:

- A process app configured with `binary` uses `tamaya deploy` to deliver an executable and run it as a systemd service.
- A static site or SPA configured with `static_root` uses `tamaya publish` to deliver files that Caddy serves directly.

This article focuses on process apps, where Tamaya's design is most visible. A published app has no application process, dedicated Linux user, systemd sandbox, or process health check.

The deployment artifact for a process app is a single executable built for the worker's Linux environment and CPU architecture. Tamaya does not build that binary or install source code and language package managers on the worker. It uploads a finished artifact produced locally or in CI directly as a release.

```toml
name = "api"
worker = "tamaya-prod"
binary = "./dist/api"
domain = "api.example.com"
```

An application must honor the following runtime contract:

- Listen for HTTP traffic on the `PORT` provided by Tamaya.
- Store persistent data in `TAMAYA_DATA_DIR`, or in the service user's home directory, which points to the same location.
- Return success from the configured health endpoint.
- Bundle any runtime dependency that does not exist on the worker, or provision it in advance.

Tamaya also provides `HOSTNAME=127.0.0.1`, so applications that use this value as their listening address bind to localhost. This is not enforced through a network namespace or firewall, however. Tamaya cannot prevent an application from ignoring the environment variable and binding to `0.0.0.0`. Treat `PORT`, `HOSTNAME`, and `TAMAYA_DATA_DIR` as reserved names: Tamaya does not currently reject them in `tamaya env`, and duplicate values in the worker environment file override the generated unit values.

## Why Not Wrap the Application in a Container Image?

The question is not whether binaries are always superior to containers. It is **which deployment unit makes more sense when you already have a self-contained executable**.

[Kamal](https://github.com/basecamp/kamal) is a broader tool that deploys Docker-packaged applications over SSH to one or more machines. Its [standard deployment](https://kamal-deploy.org/docs/commands/deploy/) builds an image, pushes it to a registry, pulls it onto the server, and has kamal-proxy verify that the new container responds before switching traffic. The registry need not be an external service; a [local registry](https://kamal-deploy.org/docs/configuration/docker-registry/) is also supported.

| Dimension | Kamal | Tamaya |
| --- | --- | --- |
| Delivery unit | Docker image | Linux executable |
| Build | The normal deployment flow includes an image build | Does not build; accepts a finished binary |
| Distribution path | Push to a registry, then pull from each server | Upload directly to the worker over SSH |
| Worker runtime | Docker and kamal-proxy | systemd and Caddy |
| Primary scope | Multiple servers and roles, containerized apps, and [accessories](https://kamal-deploy.org/docs/configuration/accessories/) | Self-contained process apps on a single worker |

Tamaya deployments do not require a Dockerfile, image registry, or Docker daemon, nor do they introduce Docker image, container, and network state. The running state is visible in systemd, logs live in the journal, and Caddy is the entry point, leaving a short diagnostic path on the host.

On the other hand, a container is a more natural fit when an application needs many OS packages, a dynamic-language runtime, a headless browser, multiple processes, or an existing Docker image, because it can package the userspace along with the application. Kamal's wider scope is also a better fit when multiple servers and roles, MySQL, Redis, and other services should share one operational model.

> Use a container when the userspace belongs in the artifact. Use a binary when the application itself is already the complete artifact.

Choosing a binary does not eliminate dependencies or supply-chain risk. When a binary uses shared libraries and `verify_binary_deps = true` is enabled, Tamaya runs `ldd` after upload and before startup, when it is available on the worker, and checks its output for known patterns indicating missing `.so` files. If `ldd` is unavailable, the check is skipped. It does not establish the artifact's overall safety or ABI compatibility.

## Give Releases and Data Different Lifetimes

Tamaya manages the following worker-side directory structure for each process app:

```text
/var/lib/tamaya/apps/<app>/
├── releases/
├── current
├── previous
├── data/
└── metadata.toml
```

The per-application operation lock is stored separately at `/var/lib/tamaya/app-locks/<app>.lock`; port allocation uses the worker-wide `/var/lib/tamaya/ports.lock`.

Each deployment creates a new directory under `releases/` and places the executable there as `app`. The systemd unit's `ExecStart` and `WorkingDirectory` reference that release's specific path. `current` and `previous` are symlinks for operators and metadata; a running unit does not follow the mutable `current` symlink.

Releases are read-only by default. Applications that persist a SQLite database, uploads, caches, or similar state use the separate `data/` directory. This directory survives deployments, rollbacks, and a normal `tamaya delete`; only `tamaya delete --purge` removes it.

For cases such as self-extracting binaries that must write runtime assets beside the executable, `writable_release = true` makes the release writable. The binary itself remains owned by root with mode `0755`. For ordinary applications, using `TAMAYA_DATA_DIR` instead of making releases mutable preserves a clearer boundary between updates and persistent state.

> Releases are replaceable; data persists. Separating their lifetimes is the foundation of a rollback-capable deployment.

## Create Per-Application Boundaries in systemd Units

On the first deployment, Tamaya creates a system user named `tamaya-<app>`. It has no login shell, and its home directory points to the application's `data/` directory. Every release runs in a systemd unit whose `User=` directive names this dedicated user.

Generated units include the following sandboxing directives:

| Directive | Purpose in Tamaya |
| --- | --- |
| `NoNewPrivileges=yes` | Prevents the process from gaining privileges through mechanisms such as setuid or capabilities |
| `ProtectSystem=strict` | Makes the host's principal filesystems read-only by default |
| `ProtectHome=yes` | Hides `/home`, `/root`, and `/run/user` from the service |
| `PrivateTmp=yes` | Gives each service private `/tmp` and `/var/tmp` directories |
| `ReadWritePaths=.../data` | Restricts the default persistent write location to that application's `data/` directory |

`ReadWritePaths` creates a writable exception on a host made read-only by `ProtectSystem=strict`; it does not make other applications' directories invisible. Whether one app can read another app's `data/` also depends on ordinary ownership and file modes, so applications must avoid creating data with overly permissive modes.

When `[memory].max` and `[cpu].quota` are configured, Tamaya passes them to systemd as `MemoryMax=` and `CPUQuota=`, respectively. systemd and cgroup v2 enforce the actual usage limits; Tamaya's role is to add the configured values to the unit. Limits are not inferred automatically, and omitted limits are not added.

```toml
[memory]
max = "512M"

[cpu]
quota = "50%"
```

These controls create useful per-application boundaries, but they do not provide isolation equivalent to a VM or microVM. Applications share the same host kernel, filesystem, network stack, loopback interface, and Caddy instance. Compromise of the host root account or kernel can affect the entire worker, so multi-tenant environments that run untrusted third-party code require a stronger isolation boundary.

## Treat the Caddy Route as Part of a Release

A process app receives an allocated `PORT` and `HOSTNAME=127.0.0.1`. When `domain` is configured and the application follows this contract by listening on loopback, Caddy accepts external HTTP(S) traffic.

```text
Internet -> Caddy :80/:443 -> 127.0.0.1:<allocated-port> -> app
```

Tamaya generates a Caddy route from the `domain` and optional `path` in `.tamaya.toml`. Caddy handles reverse proxying and, for ordinary HTTPS domains, TLS certificates; Tamaya controls which application release receives the traffic. Explicitly setting `domain = "http://example.com"` selects plain HTTP, with TLS terminating elsewhere, such as at an upstream proxy. The host operator remains responsible for ensuring that the VPS firewall does not expose the application's listening port externally.

This division of responsibility removes the need to edit Caddy configuration manually for every deployment. Routes are incorporated into the release-switching procedure alongside worker metadata, and public traffic does not move merely because the process started successfully.

## Switch Traffic Only After a Successful Health Check

A Tamaya process deployment proceeds in this order:

1. Upload the binary into a new release directory.
2. Allocate an available port and configure it, together with `HOSTNAME=127.0.0.1`, in a release-specific systemd unit.
3. Start the new release in a separate unit while the old release continues to run, if it was already running.
4. Wait for the configured HTTP health check to succeed.
5. After success, update the worker metadata and, if a `domain` is configured, switch the Caddy route to the new release.
6. Stop the old unit only after the route update succeeds.

If upload, startup, the health check, or the Caddy reload fails, Tamaya stops and removes the new release while preserving the previous route. It composes a blue-green deployment from release-specific units and Caddy routes without installing a dedicated resident controller or cluster scheduler.

`tamaya rollback` is more than a symlink swap. It starts the previous release on a new port, waits for its health check to succeed, switches the Caddy route back when a `domain` is configured, and then stops the former current unit.

The guarantees are deliberately narrow:

- A health check establishes only that the configured HTTP endpoint succeeds at the moment traffic is switched.
- It does not guarantee the health of background jobs, every route, or external services.
- Tamaya does not detect application failures after traffic begins and roll back automatically; systemd restart behavior responds first to process failures.
- Rollback restores the application release and route. It does not revert database migrations, the contents of `data/`, environment values stored on the worker, or side effects in external APIs.
- For a short period during deployment, the old and new releases can both access the same `data/`, so schema changes and concurrent writes must remain compatible.

"Preserve the old route if the new release fails before the switch" and "recover automatically from failures after the switch" are different guarantees. Tamaya provides the former, together with an explicit operator-initiated rollback.

## A Small Configuration File and CLI Make Operations Observable

Tamaya's project contract consists of a small `.tamaya.toml`, a prebuilt artifact, and an OpenSSH worker alias. Unknown configuration keys are rejected, and the corresponding CLI options for worker, binary, domain, path, and other settings override the project configuration. Binary dependency verification can also be enabled with a CLI option.

Routine operations are expressed through a short CLI:

```bash
tamaya deploy --dry-run
tamaya deploy
tamaya status
tamaya logs
tamaya rollback
```

Because the worker-side `metadata.toml` is the source of truth for deployment state, every CLI session reads the same current release, route, and port. `tamaya status` displays this state, and `tamaya logs` reads the current release's journal. The Tamaya CLI is not a resident controller; it connects over SSH only when needed to move this state forward.

This compact model is also advantageous for AI agents. Tamaya's bundled [agent skill](https://github.com/bhbs/tamaya/tree/main/skills/tamaya) instructs an agent to inspect the project's manifest, build configuration, artifacts, and health route first, and to ask a person only for details that cannot be inferred, such as the domain or SSH worker. This lets Tamaya distribute a product-specific investigation and confirmation workflow alongside the tool, not just a configuration schema and validator.

There is an important limit: `tamaya deploy --dry-run` does not connect to the worker. It displays the resolved local configuration, but does not validate worker readiness, CPU architecture, shared libraries, actual startup, or route updates. Tamaya also does not build or cross-compile the application. An AI producing a valid configuration is not the same as producing a valid artifact.

## Secret Handling and Authorization Boundaries Are Separate Concerns

Application secrets belong on the worker through `tamaya env`, not in `.tamaya.toml`.

```bash
tamaya env set DATABASE_URL
tamaya env list
tamaya env unset DATABASE_URL
```

`set` supports interactive input, and automation can supply a value through standard input. `list` displays key names only; stored values do not appear in normal CLI output. Values are stored on the worker as plaintext in `/etc/tamaya/apps/<app>.env`, a root-owned file with mode `0600`, and systemd reads them whenever it starts the service. Changes do not propagate to the already-running process. The supported Tamaya CLI path is to deploy a new release; the updated values take effect the next time the unit starts, including a systemd restart after a failure.

This approach reduces the chance of accidentally exposing values in a repository, shell history, or an AI conversation. It is not an authorization boundary against AI, however. The Tamaya SSH deployment user requires root access or passwordless sudo to create application users, install systemd units and environment files, control services, and update Caddy. Compromise of that SSH credential should be treated as a host-level compromise.

Even when an AI agent generates configuration and reviews dry-run output, production deployments, environment changes, stopping or deleting an app, and especially `delete --purge`, which destroys persistent data, should remain subject to human approval. Tamaya provides neither a dedicated consolidated audit log nor fine-grained, per-action authorization comparable to cloud IAM.

## What Tamaya Does—and Does Not—Manage

Tamaya keeps its scope deliberately narrow. Blurring that boundary leads to confusing "can deploy an app" with "can operate a server."

| Area | What Tamaya manages | What the operator or another system manages |
| --- | --- | --- |
| Worker preparation | `setup` installs some prerequisites, prepares Tamaya directories and the Caddy import, and `check` inspects worker readiness | VPS procurement, OS image, networking, and initial SSH configuration |
| Application build | Accepts a prebuilt binary or static files | Build, test, cross-compilation, artifact signing, and SBOM generation |
| Runtime dependencies | For process apps, can optionally check known missing-`.so` patterns when `ldd` exists on the worker | OS package selection, installation, updates, and compatibility |
| Process lifecycle | Application user, systemd unit, start, restart, stop, and journal access for a process app | Security updates for the OS and systemd, and host reboot planning |
| HTTP routing | Configuration and switching of Tamaya-managed Caddy routes | DNS records, upstream load balancers, and firewall or security-group rules |
| Releases | Manages `current` / `previous`; rollback uses health checks for process apps and route switching for published apps | Database migrations, data rollback, environment-value versioning, and compensation for external side effects |
| Persistent data | Preserves a process app's `data/` across deployments | Backups, encryption, off-host storage, restore testing, and replication |
| Secrets | Places a per-process-app environment file with a restrictive mode | Secret issuance, rotation, revocation, and key management in a separate failure domain |
| Observability | Provides `status` and, for process apps, journal logs for the current unit | External monitoring, metrics, alerting, long-term log storage, and a consolidated audit trail |
| Availability | Switches routes on one worker after a health check for process apps, or without one for published apps | Multi-node failover, horizontal autoscaling, and multiple regions |

Tamaya v1 does not provide containers, Firecracker, KVM, managed databases, or build automation. It can keep configuration and operations small precisely because it targets the narrow case of running single-binary processes on one systemd worker.

## Tamaya's Value Is in How It Combines the Pieces

Creating a Linux user, writing a systemd unit, and configuring a Caddy reverse proxy can each be done by hand. But when every release needs a distinct unit name and port, routes and metadata must move together only after a health check, the old route must survive a failure, and ownership of `data/` and the environment file's mode must be set consistently, the procedure itself becomes operations software.

Tamaya turns Linux primitives into consistent defaults that narrow each application's privileges and writable surface. It also models deployment as a state transition with a defined order and failure behavior, using worker metadata as the source of truth that the CLI can observe and modify. Linux provides the parts; Tamaya provides the deployment convention and execution procedure for deploying self-contained binaries directly to a systemd worker.

For setup instructions, see [Quick Start](../guide/quickstart.md). For scope and limitations, see [Caveats](../guide/caveats.md).

**Build a binary. Deploy a binary.**

## References and Primary Sources

The Tamaya-specific descriptions in this article are based on the following project documentation and implementation sources (last verified July 31, 2026). Implementation details are pinned to Tamaya v0.1.2, commit `f195d2d`.

### Tamaya

- [Architecture](../guide/architecture.md)
  The SSH-operated worker model, worker metadata, dedicated application users, the release and data layout, and systemd sandbox design.
- [Configuration](../guide/config.md)
  Process apps and published apps, `.tamaya.toml`, health checks, resource limits, and domain and path configuration.
- [Deploy](../guide/deploy.md)
  Binary upload, dependency checks, port allocation, health checks, Caddy switching, and the scope of dry runs.
- [Blue-Green Deploy](../guide/blue-green.md)
  `current` / `previous`, failures before the traffic switch, explicit rollback, and differences from published apps.
- [Environment Variables and Secrets](../guide/environment.md)
  `tamaya env`, storage on the worker, when changes take effect, and the privilege boundary of the SSH deployment user.
- [Caveats](../guide/caveats.md)
  The single worker, shared kernel, health checks, persistent data, secret guarantees, and operator responsibilities.
- [Publish](../guide/publish.md)
  Static and SPA delivery, direct serving through Caddy, and route-only rollback behavior.

For the Tamaya v0.1.2 implementation, see [`app.service`](https://github.com/bhbs/tamaya/blob/f195d2dd3a2f9810fce2c28920abd52513fb2652/crates/tamaya-cli/src/scripts/app.service#L5-L19) for generated units; [`deploy.sh`](https://github.com/bhbs/tamaya/blob/f195d2dd3a2f9810fce2c28920abd52513fb2652/crates/tamaya-cli/src/scripts/deploy.sh#L24-L120) and [`rollback.sh`](https://github.com/bhbs/tamaya/blob/f195d2dd3a2f9810fce2c28920abd52513fb2652/crates/tamaya-cli/src/scripts/rollback.sh#L24-L142) for deployment and rollback ordering and failure handling; and [`env-set.sh`](https://github.com/bhbs/tamaya/blob/f195d2dd3a2f9810fce2c28920abd52513fb2652/crates/tamaya-cli/src/scripts/env-set.sh#L13-L26) for environment-file placement.

### Comparison Target: Current Kamal Documentation

The following rolling documentation was accessed July 31, 2026, when [Kamal v2.12.0](https://github.com/basecamp/kamal/releases/tag/v2.12.0) was the latest release.

- [Kamal — Installation](https://kamal-deploy.org/docs/installation/)
  The standard setup flow, including SSH access, Docker preparation, building and distributing an image, and traffic switching through kamal-proxy.
- [Kamal — Deploy](https://kamal-deploy.org/docs/commands/deploy/)
  The sequence of image building, registry distribution, container startup, response verification, and traffic switching in `kamal deploy`.
- [Kamal — Docker Registry](https://kamal-deploy.org/docs/configuration/docker-registry/)
  External and local registry configuration.
- [Kamal — Servers](https://kamal-deploy.org/docs/configuration/servers/)
  Multiple-server and role configuration.
- [Kamal — Accessories](https://kamal-deploy.org/docs/configuration/accessories/)
  Management of containers other than the main service, such as databases and Redis.
