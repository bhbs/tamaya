# Why Tamaya?

## Deploy Apps, Not Containers

Software has become dramatically easier to build. AI-assisted development, modern frameworks, and better tooling allow a single developer to create and ship more applications than ever before.

Deployment has not kept pace. Tamaya is built for developers who want to run many small, self-contained applications on one VPS without adopting a container platform.

## The Problem

### Cloud Infrastructure Is Expensive for Small Applications

Many cloud platforms charge per running instance. For small applications, cold starts are often unacceptable, which means keeping instances running around the clock. Databases introduce additional costs, while simple deployment models such as SQLite are often discouraged or unsupported.

As the number of applications grows, infrastructure costs grow with it.

### Running Many Apps on One VPS Sacrifices Isolation

Running multiple applications on a single VPS is cost-effective. However, applications increasingly depend on large dependency trees, AI-generated code, and rapidly evolving frameworks.

Supply-chain attacks and remote code execution vulnerabilities mean that a single compromised application can become a serious risk. Developers need stronger isolation between applications without paying for a dedicated VM per application.

### Containers Add Operational Overhead

Containers solve many deployment problems, but they also introduce Dockerfiles, image registries, build caches, multi-architecture builds, and CI/CD optimization. Those tools are valuable when an application needs them. For many small applications, developers simply want to deploy an executable and run it.

## A New Opportunity

Several technologies have matured:

- systemd is ubiquitous on Linux and provides per-unit sandboxing, resource limits, and process supervision.
- Go and Zig make cross-compilation straightforward.
- Node.js supports Single Executable Applications (SEA).
- Modern frameworks increasingly support standalone builds.

Applications are becoming easier to package as self-contained executables. At the same time, Linux hosts already include a capable init system that can supervise processes, apply resource limits, and provide meaningful per-application isolation without requiring a container workflow.

## Tamaya

Tamaya combines these ideas. Instead of deploying containers, Tamaya deploys applications. Each application runs under a dedicated Linux user inside a hardened systemd service while sharing a single Linux VPS.

The result is a platform designed for:

- Low infrastructure cost
- Per-app Linux users and systemd sandboxing
- No required Docker workflow
- SQLite-friendly persistent storage
- Health-checked blue-green deploys with automatic rollback
- High application density on a single server

Build a binary. Deploy a binary.

Tamaya is not trying to replace Kubernetes, PaaS platforms, or container-based production systems. It is designed for the simpler case: one developer, one VPS, many small services.

Tamaya is intentionally focused on self-contained applications running on a single Linux VPS. Read [Caveats](./caveats.md) for the current scope and tradeoffs.
