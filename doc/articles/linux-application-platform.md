---
lang: en-US
title: How Far Can Linux Go as an Application Platform Without Containers?
description: A layered look at the roles of the Linux kernel, systemd, reverse proxies, and SSH—and at the capabilities and limits of applying host-level isolation to multiple applications without containers.
---

# How Far Can Linux Go as an Application Platform Without Containers?

When people hear “running multiple applications on a single VPS,” they may picture starting processes from a shell and restarting them by hand when they crash. Modern Linux, however, already provides mechanisms for separating process privileges, limiting resource usage, narrowing filesystem visibility, and supervising services.

That does not make Linux services equivalent to containers. The kernel provides the building blocks for establishing boundaries, while systemd is a userspace service manager that configures those building blocks per service. Publishing HTTP also requires a reverse proxy; administering the server requires SSH; and replacing artifacts safely requires a deployment procedure.

This article separates those responsibilities and examines how far a single Linux server can serve as an application platform—and where its limits lie.

## Separating the Layers of a Linux Application Platform

Viewed as an application platform, a single Linux server consists of the following layers.

| Layer | Primary responsibility | Representative mechanisms |
| --- | --- | --- |
| Linux kernel | Enforce privileges and resource limits; isolate what processes can see | UID / GID, file permissions, capabilities, cgroups, namespaces, seccomp |
| systemd | Start, stop, and supervise services; configure kernel mechanisms per service | Units, restart policies, resource controls, sandboxing directives, journal integration |
| Reverse proxy | Public HTTP(S) entry point, TLS, host- and path-based routing | Caddy, Nginx, and others |
| Administrative access | Authenticated remote control of the server | OpenSSH and others |
| Deployment mechanism | Artifact placement, traffic switching, health checks, rollback | Scripts or deployment tools |

These distinctions matter. systemd does not issue TLS certificates, and Caddy does not isolate file permissions between applications. The ability to connect over SSH does not make a release switch safe. Even the phrase “run it directly on Linux” encompasses several pieces of software and a set of operational contracts.

## Three Primary Dimensions

The following is not an exhaustive taxonomy of Linux isolation mechanisms. Linux also offers capabilities, seccomp filters for restricting system calls, and Linux Security Modules such as SELinux and AppArmor. This article focuses on three foundations for colocating applications: privileges, resource allocation, and visibility.

### Users and File Permissions: Who Does the Process Run As?

The Linux kernel authorizes access by comparing a process's UID and GID with file ownership, mode bits, ACLs, and related metadata. The usernames in `/etc/passwd` are human-readable mappings; the kernel enforces access using numeric IDs.

Assigning a dedicated user to each application can prevent one compromised application from directly reading another application's secrets or data. There is no need to run every application as root. If an application needs only a specific privilege, such as binding to a low-numbered port, a narrowly scoped Linux capability can be granted instead of full root privileges.

Creating dedicated users alone does not complete the isolation boundary. World-readable files, shared groups, writable executables, or excessive capabilities can undermine it. User separation becomes meaningful only when paired with deliberate ownership and permission design.

### cgroups: How Many Resources May a Process Use?

[cgroup v2](https://docs.kernel.org/admin-guide/cgroup-v2.html) is a kernel mechanism that organizes processes into a hierarchy of groups and manages resources such as CPU, memory, I/O, and process counts. For example, setting memory limits or CPU weights for one application can reduce the risk that a runaway process affects the entire host.

cgroups answer “how much may this process use?”, not “what may this process see?” Limiting an application to 512 MB of memory does not hide files or network interfaces. cgroups are essential for capacity protection, but they are not a security boundary by themselves.

### Namespaces: What Can a Process See?

[Linux namespaces](https://man7.org/linux/man-pages/man7/namespaces.7.html) give processes different views of global resources such as mount points, process IDs, network stacks, hostnames, IPC facilities, and user IDs. Container isolation also relies heavily on these namespaces.

Namespaces and cgroups are separate mechanisms. A network namespace separates the visibility of sockets and routing tables but does not limit CPU usage. A cgroup can limit CPU usage but does not hide ports within the same network namespace. Privileges, resources, and visibility must be combined according to the relevant threat model.

Nor does creating an ordinary systemd service automatically place it in every kind of namespace. By default, services share the host kernel, network, loopback interface, and much of the filesystem with other processes. Any required separation must be configured explicitly.

## systemd Turns Kernel Mechanisms into Services

systemd is not part of the kernel; it is a service manager that runs on Linux. It reads unit files, starts processes as designated users, tracks their exit states, and handles processes belonging to the same unit together when stopping the service. As the [systemd architecture](https://systemd.io/ARCHITECTURE/) explains, the service manager applies configured sandboxes, namespaces, cgroups, and other controls before starting a process. Service lifecycles are defined in [`systemd.service(5)`](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html), execution environments and sandboxes in [`systemd.exec(5)`](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html), and cgroup configuration in [`systemd.resource-control(5)`](https://www.freedesktop.org/software/systemd/man/latest/systemd.resource-control.html).

For example, the core of a unit that runs a self-contained web application might look like this:

```ini
[Unit]
After=network-online.target
Wants=network-online.target

[Service]
User=notes
Group=notes
ExecStart=/srv/apps/notes/current/notes
WorkingDirectory=/var/lib/notes
EnvironmentFile=/etc/notes.env
Restart=on-failure

NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
ReadWritePaths=/var/lib/notes/data

MemoryMax=512M
CPUQuota=100%

[Install]
WantedBy=multi-user.target
```

In this example, systemd brings the following responsibilities together:

- `User=` and `Group=` determine the execution identity.
- `ExecStart=` declares what to start and lets systemd track its exit state.
- `Restart=` defines the restart policy after an abnormal exit.
- `ProtectSystem=`, `ProtectHome=`, and `PrivateTmp=` use kernel mechanisms, including mount namespaces, to narrow what the process can see and write.
- `NoNewPrivileges=` prevents the process and its children from gaining privileges through `execve`.
- `MemoryMax=` and `CPUQuota=` configure cgroup resource controls.

`CPUQuota=100%` represents the execution time of one logical CPU, not 100% of all CPUs on the host. For example, `200%` permits up to two CPUs' worth of execution time.

The ability to configure these controls does not mean they have been configured safely. Support for sandboxing directives depends on the versions of systemd, the kernel, and the distribution. Restricting system calls or file access beyond what an application requires can also prevent it from starting. Controls should be introduced incrementally and tested against the real workload. `systemd-analyze security` is a useful prompt for reviewing a unit, but it does not prove that the application or the overall configuration is secure.

The same caveat applies to logging. In many systemd configurations, an application that writes to stdout and stderr integrates with systemd-journald, and its output can be inspected per unit with a command such as `journalctl -u notes.service`. The journal, however, is neither external availability monitoring nor a centralized, tamper-resistant audit log. Whether to make storage persistent, how long to retain logs, and whether to forward them off-host for resilience are separate design decisions.

## Publishing a Web Service Requires Another Layer

Starting an application with systemd does not configure domains, TLS, or HTTP routing. A reverse proxy such as Caddy or Nginx therefore sits at the public entry point.

```text
                         systemd
                    ┌─ notes.service ── 127.0.0.1:4101
Internet ──> Caddy ─┼─ api.service ──── 127.0.0.1:4102
  :80/:443          └─ bot.service ──── 127.0.0.1:4103

Operator ──> SSH ───── host administration / deployment
```

Caddy can select an upstream based on the request's domain or path, provide a [reverse proxy](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy), and manage [automatic HTTPS](https://caddyserver.com/docs/automatic-https). Having each application listen only on a loopback address removes the path for direct Internet connections to application ports and generally consolidates public exposure on ports 80 and 443.

However, `127.0.0.1` is not a security boundary within the host. Another application in the same network namespace can connect to that port as well. Preventing or authenticating inter-application communication requires separate measures such as network namespaces, packet filtering, or application-level authentication.

SSH is also not a kernel feature; it is separate software that provides an administrative access path. A single entry point is not safe by itself. Operators must configure public-key authentication, source restrictions, key revocation, and sudo policies; review connection and authentication logs; and design separate command auditing when required. [`sshd_config(5)`](https://man.openbsd.org/sshd_config) documents access controls for an OpenSSH server, while [`sshd(8)`](https://man.openbsd.org/sshd) describes the daemon's authentication and session handling, along with configuration validation. In particular, if a deployment key can use passwordless sudo or log in as root, its disclosure should be treated as a compromise of the entire host.

## A Practical Model for Colocating Applications

When operating multiple applications on one server, defining ownership boundaries and units of change matters more than merely deciding how many processes to start. At a minimum, separate the following:

- An unprivileged Linux user for each application
- Release directories that retain previous generations instead of being overwritten
- Writable data kept outside releases and preserved across deployments
- Secret and environment files kept out of repositories and unit files
- Non-conflicting loopback ports and reverse-proxy routes
- Per-unit restart policies, resource limits, and sandboxing directives
- Logs attributable to individual units and health monitoring from outside the host

The directory layout can likewise separate responsibilities without depending on any particular tool:

```text
app/
├── releases/<release-id>/  # artifact treated as immutable
├── current -> releases/... # active release
└── data/                   # persistent writable data

/etc/<app>.env             # restricted configuration stored outside releases
```

systemd's responsibility ends at running a designated release under designated constraints. It does not automatically transfer a new release, verify binary compatibility, perform a health check, switch routes, stop an old release, or roll back. Avoiding reliance on a manual sequence requires a separate deployment procedure or tool.

A directly deployed executable is not necessarily completely self-contained. If it depends on dynamically linked shared libraries, a particular CPU architecture or glibc version, or external commands, compatibility with the worker must be verified. It is safer to produce artifacts in a reproducible build environment and state their dependency requirements explicitly than to build them ad hoc on the production host.

## What Containers Add

Containers do not each boot a separate kernel. A typical Linux container shares the host kernel, and its runtime starts processes using a combination of namespaces, cgroups, capabilities, mounts, and related mechanisms. The [Docker security overview](https://docs.docker.com/engine/security/) likewise explains that Docker creates namespaces and control groups when it starts a container.

What matters is not merely that containers use kernel mechanisms. An image bundles the application with a userspace and root filesystem, while the container runtime and surrounding tooling provide a common model for networks, mounts, process lifecycles, and distribution.

| Dimension | Direct execution as a systemd service | Typical Linux container |
| --- | --- | --- |
| Artifact | An executable or application files, which may depend on host libraries | An image bundling the application with a userspace / root filesystem |
| Filesystem | Based on the host filesystem, with selected portions restricted by unit configuration | A dedicated root filesystem assembled from image layers and mounts |
| Process / network | Shared with the host by default; required isolation is explicit | The runtime commonly creates a set of namespaces |
| Resource control | systemd manages the unit's cgroup | A container runtime or orchestrator manages cgroups |
| Lifecycle | systemd units and release management on the host | Managed through image, container, registry, and runtime tooling |
| Kernel | Shares the host kernel | Shares the host kernel |

A sandboxed systemd service is therefore not identical to a container. Direct execution uses the host userspace and omits an image format, registry, and container network. That is both the source of its simplicity and one of its constraints.

For a self-contained executable, distributing it directly instead of wrapping it in an image can be a reasonable choice. If an application requires many OS packages, a language runtime, a headless browser, an existing image, or multiple supporting processes, a container can be more reproducible because it packages the required userspace into the artifact. Neither model eliminates compatibility requirements for CPU architecture or the host kernel.

## The Limits of a Shared Kernel

Linux users and file permissions, systemd sandboxes, and cgroups are practical ways to reduce interference among applications operated by the same trusted administrator. They do not, however, provide the same isolation boundary as independent virtual machines.

- A kernel vulnerability or compromise of host root can affect every colocated application.
- Unless network namespaces are separated, applications share loopback and the host network stack.
- Even with separate mount namespaces, applications share the same kernel and physical resources.
- Without cgroup limits, one application may exhaust memory or process counts.
- Even with cgroups configured, free space on shared filesystems and host-wide I/O require separate management.
- Keeping local data outside release directories does not protect it from disk failure or deletion on the host.

For untrusted third-party code, potentially malicious plugins, or strong tenant isolation between customers, consider boundaries stronger than ordinary process isolation on a shared kernel: separate hosts, VMs, microVMs, or sandboxed container runtimes. Ordinary containers share the host kernel as well, so “put untrusted code in a container” is not necessarily sufficient. The workload's threat model should determine the required boundary.

## Linux Does Not Operate Itself

Restarting processes with systemd does not automate all server operations. Running production on a single Linux server still leaves at least the following responsibilities:

- Updating the OS, kernel, systemd, reverse proxy, and application dependencies
- Configuring the firewall, SSH, sudo, and exposed ports, and managing keys
- Monitoring and alerting on disk, memory, CPU, processes, certificates, and external health
- Defining journal retention and, when needed, forwarding logs off-host
- Backing up persistent data and configuration to a separate failure domain, and testing restores
- Maintaining reproducible procedures for release placement, health checks, traffic switching, and rollback
- Verifying recovery after host reboots and planning capacity

A single VPS is also a single failure domain. systemd can restart a process after it fails, but it cannot fail over to another machine when the host, disk, network, or data center becomes unavailable. If downtime is unacceptable, the design needs additional components such as multiple hosts, an external load balancer, and data replication.

## When Is This Enough?

A container-free Linux platform is a good fit when the following conditions hold:

- The applications are operated by the same developer or lie within the same trust boundary.
- Each application runs as a self-contained executable or has only a small set of explicit host dependencies.
- All workloads fit within the CPU, memory, disk, and bandwidth of one machine.
- The recovery time after host failure is acceptable, and recovery from an external backup is possible.
- The operator is prepared to own OS patching, monitoring, keys, and backups.

Within those constraints, Linux is more than a foundation on which to install a container runtime. Its kernel boundaries and systemd's service management can be used directly to build an application platform with few components and an understandable operational model.

Conversely, use containers when userspace dependencies should be fixed in an image; consider a separate host, VM, microVM, or sandboxed runtime when untrusted workloads require stronger isolation; and add multiple nodes or managed services when the loss of one machine is unacceptable. The question is not which technology is universally superior, but how many layers to add for the required levels of packaging, isolation, and availability.

For the cost, performance, and security implications of the single-VPS operating model itself, see [Why Indie Developers Should Consider a Single VPS](./why-single-vps.md). To see how the Linux building blocks described here can be formalized into a release lifecycle, continue to [How Tamaya Turns a Linux Server into a Deployment Platform](./how-tamaya-uses-linux.md).

## References and Primary Sources

The technical explanations in this article draw primarily on the following upstream documentation and specifications (last reviewed July 31, 2026). Support for systemd directives varies by installed version; always consult the man pages on the target host as well.

### Linux Kernel and Process Isolation

- [Control Group v2 — Linux Kernel Documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html)
  The official specification for the cgroup v2 hierarchy and the controllers that manage CPU, memory, I/O, process counts, and other resources.
- [namespaces(7) — Linux man-pages](https://man7.org/linux/man-pages/man7/namespaces.7.html)
  Describes the resources isolated by mount, PID, network, UTS, IPC, user, and other namespaces.
- [capabilities(7) — Linux man-pages](https://man7.org/linux/man-pages/man7/capabilities.7.html)
  Defines Linux capabilities, which divide root privileges into discrete units, including `CAP_NET_BIND_SERVICE` for binding to low-numbered ports.

### systemd

- [systemd.service(5)](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html)
  The official reference for service units, `ExecStart=`, `Restart=`, process supervision, and service lifecycles.
- [systemd.exec(5)](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html)
  Specifies execution environments and sandboxing directives such as `User=`, `Group=`, `NoNewPrivileges=`, `ProtectSystem=`, `ProtectHome=`, `PrivateTmp=`, and `ReadWritePaths=`.
- [systemd.resource-control(5)](https://www.freedesktop.org/software/systemd/man/latest/systemd.resource-control.html)
  Explains how systemd configures cgroup resource controls such as `MemoryMax=` and `CPUQuota=`, including the meaning of percentage values.
- [systemd-analyze(1)](https://www.freedesktop.org/software/systemd/man/latest/systemd-analyze.html)
  Documents the unit settings evaluated by `systemd-analyze security` and why its score should not be treated as a complete security assessment.
- [systemd-journald.service(8)](https://www.freedesktop.org/software/systemd/man/latest/systemd-journald.service.html)
  Describes journald's role in collecting service stdout and stderr, storing journal data, and forwarding logs.

### Publishing HTTP and Remote Administration

- [reverse_proxy — Caddy Documentation](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy)
  The official reference for Caddy's reverse proxy features, including upstream selection, HTTP forwarding, header handling, and health checks.
- [Automatic HTTPS — Caddy Documentation](https://caddyserver.com/docs/automatic-https)
  Explains certificate issuance and renewal, redirects from HTTP to HTTPS, and the conditions under which those features are enabled.
- [sshd_config(5) — OpenSSH manual](https://man.openbsd.org/sshd_config)
  Documents OpenSSH server access controls, including public-key authentication, user and source restrictions, root login, and revoked keys.
- [sshd(8) — OpenSSH manual](https://man.openbsd.org/sshd)
  Describes OpenSSH daemon authentication and session processing, along with configuration and host-key validation using `sshd -t`.

### Containers and Images

- [Docker Engine security](https://docs.docker.com/engine/security/)
  Explains Docker's use of namespaces, cgroups, and capabilities, as well as security boundaries involving containers and the Docker daemon.
- [What is a container? — Docker Documentation](https://docs.docker.com/get-started/docker-concepts/the-basics/what-is-a-container/)
  Explains that a Linux container is an isolated process and shares the kernel with other processes on the same host.
- [What is an image? — Docker Documentation](https://docs.docker.com/get-started/docker-concepts/the-basics/what-is-an-image/)
  Explains that images include applications, binaries, libraries, and configuration and are composed of filesystem layers.
- [OCI Runtime Specification — Linux-Specific Configuration](https://specs.opencontainers.org/runtime-spec/config-linux/)
  The vendor-neutral specification for the root filesystem, mounts, processes, namespaces, cgroups, capabilities, and other settings handled by a container runtime.
- [OCI Image Format Specification](https://specs.opencontainers.org/image-spec/)
  The standard defining container-image manifests, configuration, filesystem layers, and conversion to a runtime bundle.
