# TODO

## Completed

- [x] Phase 0: CLI/State, config, init, XDG paths, registry, locks
- [x] Phase 1: Remote worker foundation, SSH runner, capability checks
- [x] Phase 2: Firecracker boot on Linux worker
- [x] Phase 3: VM lifecycle (ps / stop / logs)
- [x] Phase 4: Blue-green deploy with health check + Caddy proxy switch + drain
- [x] Phase 5: Image build (Docker export → tar artifact)
- [x] Phase 6: Logging (unified format with timestamps/levels, deploy step logging, worker-aware SSH errors)

## Phase 7: TAP Management

- [ ] Ensure `deploy_tap` (t-XXXXXXXX) is cleaned up on deploy failure
- [ ] Make `deploy_tap` name sufficiently unique across concurrent deploys
- [ ] Include `{app}-deploy` runtime state in stale TAP detection (`v cleanup`)

## Phase 8: Runtime Directory Stability

- [ ] Maintain consistent state when `{app}-deploy → {app}` rename fails
- [ ] Check alignment between remote runtime dir and local runtime state

## Phase 9: Lock Stability

- [ ] Improve lock-acquisition error messages (show which process holds the lock)
- [ ] Detect and warn on long-lived lock files

## Phase 10: Interrupted Deploy Recovery

- [ ] Strengthen auto-recovery of stale `AppStatus::Deploying` registry state
- [ ] Have `DeployCleanup` drop-guard also clean remote TAP and runtime dir when possible

## Phase 11: Health Check

- [ ] Verify assumption that old/new VMs reuse the same port (`old_port`)
- [ ] Add configurable health check timeout
- [ ] Show detailed error output on HTTP health check failures

## Phase 12: State Consistency (`v ps`)

- [ ] Cross-reference local runtime state with actual remote VM state (ssh pid check, etc.)
- [ ] Warn on any mismatch

## Phase 13: Dry-run Improvements

- [ ] Pre-validate worker connectivity and TAP creation ability during dry-run
- [ ] Verify kernel/rootfs/artifact existence during dry-run

## Phase 14: Test Coverage

- [ ] Deploy failure cleanup tests
- [ ] TAP leak verification tests
- [ ] Remote rename failure tests
- [ ] Health check failure → cleanup verification tests

## Phase 15: Rollback

- [ ] Store previous image metadata (registry already has `previous_image` field)
- [ ] Boot rollback VM
- [ ] Run health checks
- [ ] Switch proxy routing back
- [ ] Stop failed or replaced VM
- [ ] Keep DB schema rollback out of scope for the first implementation

## Phase 16: Volumes

- [ ] Implement `v volume create <app>`
- [ ] Manage ext4 data volume files or block devices on the worker
- [ ] Enforce attach policy
- [ ] Enforce single-writer volume locks
- [ ] Attach persistent data volume to the guest

## Future Work

- [ ] Backup and restore
- [ ] DB schema migration strategy
- [ ] Health check failure rollback
- [ ] Volume lock recovery
- [ ] Snapshot boot
- [ ] Metrics
- [ ] Log aggregation
- [ ] Multi-host scheduling
