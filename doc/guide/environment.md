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

Setting an existing key replaces its previous value. Key names must start with an
ASCII letter or `_` and contain only ASCII letters, digits, or `_`, as required by
systemd. `env unset` can still remove names accepted by older Tamaya versions,
such as `API-KEY`.

Values are stored as quoted, escaped, single-line `KEY="value"` entries in
systemd's EnvironmentFile format. Quotes, backslashes, `$`, backticks, tabs,
leading/trailing spaces, and UTF-8 text retain their literal values. Newlines,
NUL, Unicode byte order marks, and Unicode noncharacters are rejected.

Existing entries remain unchanged until you set or unset their keys. If an older
Tamaya version stored a value containing quotes, backslashes, or surrounding
spaces, set that key again to preserve the intended value with the new encoding.

When `.tamaya.toml` defines `name`, omit the app argument:

```bash
tamaya env set DATABASE_URL
tamaya env list
tamaya env unset DATABASE_URL
```

Setting or unsetting a value does not change an already-running application process. The supported Tamaya CLI path is to deploy a new release. systemd reads the environment file whenever it starts a release unit, so updated values also apply on any later unit restart.

Treat `PORT`, `HOSTNAME`, and `TAMAYA_DATA_DIR` as reserved names. Tamaya does not currently reject these keys, and the environment file is loaded after the generated values in the systemd unit, so setting a duplicate key overrides Tamaya's value.

## Storage

The local controller does not persist environment values. Each `env set` command streams one value over SSH and atomically installs the updated environment file on the worker as:

```text
/etc/tamaya/apps/<app>.env
```

Worker-side files are owned by `root:root` with mode `0600`, including temporary
replacement files. Environment operations share the app lifecycle lock, so
concurrent updates cannot overwrite each other's changes or race with deletion.
The updated file is atomically renamed into place; a failed update leaves the
previous file intact.

systemd reads the file while starting the service and injects the values into the application process. Each application runs as its own unprivileged `tamaya-<app>` user. It receives its own values but cannot open another application's environment file.

Environment changes apply when a systemd release unit starts.

## Security Model

This design limits accidental disclosure and prevents applications from reading each other's environment files. It does not protect secrets from host root, a full VPS takeover, a compromised application reading its own environment, or a VPS snapshot, disk image, or backup containing the plaintext environment files.

Worker-side encryption is intentionally deferred. Encryption with a key stored on the same VPS provides limited protection against host compromise or full snapshots. A future encrypted-storage design should define separate key storage and key rotation.

## Worker Privileges

Tamaya connects over SSH as a deployment user, such as `deploy`. Worker operations run in a root shell through `sudo -n sh -lc`. The SSH user must be allowed to run that shell without a password; sudo must also be installed when connecting as root. This keeps metadata reads, lock creation, application-user management, systemd operations, and Caddy updates under the same privileges. Metadata stays owned by root with mode `0600`.

The `-n` option makes sudo fail immediately if it requires a password. Standard input remains available for application binaries, published files, and environment values.

Applications do not receive sudo access. Disable direct root SSH login and SSH password login where practical. Treat compromise of the deployment user's SSH key as host-level compromise because Tamaya uses that account to perform privileged operations.
