# TODO

## Phase 0: CLI and State Management

- [x] Add `clap` based command parsing.
- [x] Add shared error handling.
- [x] Add config model.
- [x] Implement `v init`.
- [x] Create local config directory.
- [x] Create image, volume, and runtime directory settings.
- [x] Add app registry.
- [x] Store current image, previous image, volume path, port, and status.
- [x] Add app-level file locks.
- [x] Add volume-level file locks.
- [x] Prevent concurrent deploys for the same app.

## Phase 1: Minimal Firecracker Boot

- [x] Read kernel and rootfs paths from CLI.
- [ ] Start Firecracker process.
- [x] Create Firecracker API socket path.
- [x] Build read-only rootfs `virtio-blk` API request.
- [x] Build TAP networking API request.
- [ ] Boot a microVM.
- [ ] Stop and clean up a microVM.

## Phase 2: VM Lifecycle

- Implement `v ps`.
- Implement `v stop <app>`.
- Track PID, socket path, and runtime directory.
- Clean up stale runtime state.
- Implement `v logs <app>`.

## Phase 3: Volumes

- Implement `v volume create <app>`.
- Manage ext4 data volume files or block devices.
- Enforce attach policy.
- Enforce single-writer volume locks.
- Attach persistent data volume to the guest.

## Phase 4: Deploy

- Boot a new VM from an immutable image.
- Run health checks.
- Update reverse proxy routing.
- Reload reverse proxy.
- Drain the old VM.
- Stop the old VM.
- Update app registry after a successful switch.

## Phase 5: Rollback

- Store previous image metadata.
- Boot rollback VM.
- Run health checks.
- Switch proxy routing back.
- Stop failed or replaced VM.
- Keep DB schema rollback out of scope for the first implementation.

## Phase 6: Image Build

- Accept existing ext4 rootfs images first.
- Add Docker export based image creation.
- Add dedicated image builder later.

## Future Work

- Backup and restore.
- DB schema migration strategy.
- Health check failure rollback.
- Volume lock recovery.
- Snapshot boot.
- Metrics.
- Log aggregation.
- Multi-host scheduling.
