# Overview

Tamaya deploys pre-built artifacts to ordinary Linux VPS workers over SSH. Process apps run under systemd, and Caddy handles TLS, reverse proxying, and route switching.

For process apps, each deploy creates an immutable release, assigns a free localhost port, starts a release-specific systemd unit, checks the configured health endpoint defaulting to `GET /health`, switches Caddy traffic, and stops the old release. Persistent data lives in the app's `data/` directory and is exposed to the process as `TAMAYA_DATA_DIR`.

Tamaya can also publish static and SPA artifacts directly through Caddy without starting an application process.

Tamaya v1 intentionally does not manage containers, microVMs, language runtimes, databases, or application builds.

Start with the [quick start](quickstart.md).

See [configuration](config.md), [deploys](deploy.md), and [environment variables and secrets](environment.md) for the core workflow and worker privilege model.
