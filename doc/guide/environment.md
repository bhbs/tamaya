# Environment Variables and Secrets

Tamaya injects application configuration and secrets as environment variables. Set a value interactively so it does not appear in shell history or process listings:

```bash
tamaya env blog set DATABASE_URL
```

For automation, pipe a value through standard input:

```bash
printf '%s' "$DATABASE_URL" | tamaya env blog set DATABASE_URL --stdin
```

`--stdin` reads the value from standard input and strips one trailing line ending if present. Use `printf '%s'` when the value must be stored without an added newline.

List or remove configured keys:

```bash
tamaya env blog list
tamaya env blog unset DATABASE_URL
```

`env list` prints key names only. Tamaya does not print stored values during normal CLI operations.

Setting an existing key replaces its previous value. Values are stored as single-line `KEY=value` entries; newlines are rejected.

When `.tamaya.toml` defines `name`, omit the app argument:

```bash
tamaya env set DATABASE_URL
tamaya env list
tamaya env unset DATABASE_URL
```

Setting or unsetting a value does not change an already-running application process. Deploy a new release for the updated environment to take effect.

## Storage

The local controller does not persist environment values. Each `env set` command streams one value over SSH and atomically installs the updated environment file on the worker as:

```text
/etc/tamaya/apps/<app>.env
```

Worker-side files are owned by `root:root` with mode `0600`. systemd reads the file while starting the service and injects the values into the application process. Each application runs as its own unprivileged `tamaya-<app>` user. It receives its own values but cannot open another application's environment file.

Environment changes apply when a systemd release unit starts.

## Security Model

This design limits accidental disclosure and prevents applications from reading each other's environment files. It does not protect secrets from host root, a full VPS takeover, a compromised application reading its own environment, or a VPS snapshot, disk image, or backup containing the plaintext environment files.

Worker-side encryption is intentionally deferred. Encryption with a key stored on the same VPS provides limited protection against host compromise or full snapshots. A future encrypted-storage design should define separate key storage and key rotation.

## Worker Privileges

Tamaya connects over SSH as a deployment user, such as `deploy`. That user must have root access or passwordless sudo access for Tamaya's privileged worker operations: creating application users, installing systemd units and environment files, controlling services, and updating Caddy configuration.

Applications do not receive sudo access. Disable direct root SSH login and SSH password login where practical. Treat compromise of the deployment user's SSH key as host-level compromise because Tamaya uses that account to perform privileged operations.
