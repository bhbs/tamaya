---
name: tamaya
description: Create and review Tamaya deployment configuration for projects. Use when an AI agent needs to generate, update, validate, or explain `.tamaya.toml` files; prepare a project for `tamaya deploy` or `tamaya publish`; choose between process app `binary` deploys and static/SPA `static_root` publishes; configure Tamaya worker, domain, path routes, health checks, resource limits, writable releases, or binary dependency verification.
---

# Tamaya

## Workflow

When creating or updating `.tamaya.toml`, first inspect the project rather than asking for details that can be inferred:

- Look for package manifests, build configs, entrypoints, existing `dist` or release outputs, health routes, and deployment docs.
- Decide whether the project is a process app (`tamaya deploy`) or a published site (`tamaya publish`).
- Read `references/tamaya-toml.md` before writing or reviewing a config.
- Preserve existing user choices in `.tamaya.toml` unless they conflict with Tamaya's schema or the user's request.

Ask only for values that are not safely discoverable, especially the public `domain`, OpenSSH `worker` alias, intended route `path`, and final app `name` when multiple reasonable names exist.

## Config Rules

Use `.tamaya.toml` as the file name.

For process apps, write `binary` and do not write `static_root` or `publish_type`. The binary must be a Linux executable for the worker, listen on `PORT`, and return success from the configured health endpoint.

For published sites, write `static_root` and usually `publish_type`. Do not write `binary`. Use `publish_type = "spa"` for client-side routers that need `index.html` fallback; use `publish_type = "static"` or omit it for ordinary static files.

Set `path` only with `domain`. Use `domain = "http://example.com"` only when TLS terminates upstream and Caddy should serve plain HTTP without obtaining a certificate.

Do not invent unsupported keys. Tamaya rejects unknown fields.

## Output

When generating a config, make the smallest useful file for the user's project. Include optional tables such as `[health_check]`, `[memory]`, and `[cpu]` only when the project or user request justifies them.

After writing or editing `.tamaya.toml`, validate locally when feasible:

- Process app: run `tamaya deploy --dry-run` if the `tamaya` binary is available.
- Published site: run the closest available local validation command for the current CLI, or at minimum parse and inspect the file against the reference.

Report any assumptions, especially unresolved worker/domain values and build artifacts that must exist before deploy or publish.
