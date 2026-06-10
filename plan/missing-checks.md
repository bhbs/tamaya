# Plan: Extend `check.sh` to Cover All Commands Used by Scripts

## Background

`check.sh` currently validates 7 external commands (`ss`, `flock`, `curl`, `tar`, `systemctl`, `caddy`, `sudo`) plus OS and cgroup checks. The scripts in `src/scripts/` collectively depend on ~25+ external commands that are not verified, meaning a worker can pass the readiness check but still fail at runtime.

## Tiers

Not all missing commands carry the same risk. Organize into three tiers based on how likely they are to be absent and how critical they are.

### Tier 1 — Must check (likely absent on minimal/stripped images, used by core flows)

| Command | Scripts | Why risky |
|---------|---------|-----------|
| `journalctl` | health-check-failure.sh, logs.sh | Not installed on systems without systemd-journal |
| `useradd` | deploy.sh | Missing on minimal containers; deploy will fail |
| `userdel` | delete-purge.sh | Same as useradd |
| `ldd` | verify-binary-deps.sh | Missing on musl-based or static-only images |
| `grep` | setup.sh, caddy-shared.sh, metadata.sh, verify-binary-deps.sh, allocate-port.sh | Pervasively used; can be absent on stripped images |
| `sed` | verify-binary-deps.sh, caddy-shared.sh, metadata.sh, rollback.sh | Same as grep |
| `awk` | allocate-port.sh, app-units.sh, env-list.sh, env-set.sh, env-unset.sh | Same as grep |

### Tier 2 — Should check (coreutils, usually present but not guaranteed)

| Command | Scripts |
|---------|---------|
| `find` | publish.sh, delete-retain-data.sh, writable-release.sh |
| `mktemp` | metadata.sh |
| `sort` | caddy-shared.sh |
| `xargs` | deploy.sh, publish.sh |
| `seq` | allocate-port.sh, deploy.sh, rollback.sh |
| `cat` | caddy-shared.sh, metadata.sh, env-*.sh |
| `cp` | deploy.sh, publish.sh, rollback.sh |
| `mv` | caddy-shared.sh, metadata.sh, deploy.sh, publish.sh, rollback.sh, env-*.sh |
| `ln` | deploy.sh, publish.sh, rollback.sh |
| `chmod` | caddy-shared.sh, metadata.sh, deploy.sh, publish.sh, rollback.sh, writable-release.sh |
| `chown` | caddy-shared.sh, metadata.sh, deploy.sh, publish.sh, rollback.sh, writable-release.sh |
| `tee` | setup.sh, deploy.sh, rollback.sh, caddy-shared.sh, maintenance*.sh, remove-caddy.sh |
| `tail` | deploy.sh, publish.sh, rollback.sh |
| `touch` | caddy-shared.sh, metadata.sh |
| `date` | deploy.sh, publish.sh |
| `basename` | caddy-shared.sh, live-domain.sh, status.sh, remove-caddy.sh |
| `dirname` | caddy-shared.sh, live-domain.sh, maintenance-domain.sh |
| `id` | caddy-shared.sh, deploy.sh |
| `sleep` | deploy.sh, rollback.sh |

### Tier 3 — Optional check (package managers, only needed by setup.sh / verify-binary-deps.sh)

| Command | Scripts | Note |
|---------|---------|------|
| `apt-get` | setup.sh, verify-binary-deps.sh | Only on Debian/Ubuntu |
| `dnf` | setup.sh, verify-binary-deps.sh | Only on Fedora/RHEL8+ |
| `yum` | setup.sh, verify-binary-deps.sh | Only on CentOS/RHEL7 |
| `dpkg` | verify-binary-deps.sh | Only on Debian/Ubuntu |
| `apt-file` | verify-binary-deps.sh | Only on Debian/Ubuntu, optional |

These are environment-specific and already handled gracefully by the scripts (they check and fall back). Adding a WARN-level check is nice but not blocking.

## Implementation

### Step 1: Add Tier 1 commands to the core loop in `check.sh`

The existing loop at line 32-39:

```bash
for cmd in ss flock curl tar; do
```

Extend to:

```bash
for cmd in ss flock curl tar grep sed awk useradd userdel journalctl ldd; do
```

This keeps the same `command -v` pattern and FAIL/SKIP behavior already in place.

### Step 2: Add Tier 2 commands as a secondary loop with WARN severity

After the Tier 1 loop, add a new loop that prints WARN instead of FAIL. These are coreutils that are almost always present — a hard FAIL would be noisy, but a WARN helps catch truly stripped images:

```bash
warn=0
for cmd in find mktemp sort xargs seq cat cp mv ln chmod chown tee tail touch date basename dirname id sleep; do
  if command -v "$cmd" >/dev/null 2>&1; then
    echo "PASS $cmd"
  else
    echo "WARN $cmd (command not found — core operations may fail)"
    warn=1
  fi
done
```

Decide whether WARN should affect the exit code:
- **Option A**: WARN does not set `fail=1` (exit 0 if only warnings) — safer for existing workflows
- **Option B**: WARN sets `fail=1` (exit 1 on any missing command) — strictest guarantee

Recommend **Option A** for now with a `--strict` flag to opt into Option B.

### Step 3: Add Tier 3 package-manager awareness (optional)

At the end of the script, detect the package family and warn if no known package manager is available:

```bash
if command -v apt-get >/dev/null 2>&1 || command -v dnf >/dev/null 2>&1 || command -v yum >/dev/null 2>&1; then
  echo "PASS package-manager"
else
  echo "WARN package-manager (no apt-get, dnf, or yum found; setup may not be able to install dependencies)"
fi
```

### Step 4: Update the summary line

The `progress` line at the end should reflect the result tier:

```bash
if test "$fail" -ne 0; then
  exit 1
elif test "${warn:-0}" -ne 0; then
  echo "check complete with warnings"
  exit 0
fi
```

## Files to modify

- `crates/tamaya-cli/src/scripts/check.sh` — all changes go here

## Out of scope

- Adding new scripts or changing existing script logic
- Modifying the Rust-side `check` subcommand in `worker.rs` (it just uploads and runs `check.sh`)
