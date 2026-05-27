# TODO

## Phase 0: CLI and State Management

- [x] Add `clap` based command parsing.
- [x] Add shared error handling.
- [x] Add config model.
- [x] Implement `v init`.
- [x] Create local controller config directory using XDG Base Directory paths.
- [x] Create initial image, volume, state, lock, and runtime directory settings using XDG Base Directory paths.
- [x] Add app registry.
- [x] Store current image, previous image, volume path, port, and status.
- [x] Add app-level file locks.
- [x] Add volume-level file locks.
- [x] Prevent concurrent deploys for the same app.
- [x] Add worker configuration for SSH targets.
- [x] Separate Mac controller state from Linux worker paths.
- [x] Treat kernel, rootfs, runtime, and API socket paths as worker-side paths.

## Phase 1: Remote Worker Foundation

- [x] Add `--worker` selection or default worker config.
- [x] Add SSH command runner.
- [x] Add remote file/path validation helpers.
- [x] Add worker capability checks for Linux, KVM, Firecracker, networking, and permissions.
- [x] Add remote runtime path conventions.
- [x] Add remote runtime directory creation.
- [x] Add remote log directory path conventions.

## Phase 2: Minimal Firecracker Boot on Linux Worker

- [x] Read kernel and rootfs paths from CLI.
- [x] Start Firecracker process over SSH on the worker.
- [x] Create Firecracker API socket path on the worker.
- [x] Build read-only rootfs `virtio-blk` API request.
- [x] Build TAP networking API request.
- [x] Send Firecracker API requests against the worker-side socket.
- [x] Boot a microVM on the worker.
- [x] Stop and clean up a worker microVM.

## Phase 3: VM Lifecycle

- [x] Implement `v ps`.
- [x] Implement `v stop <app>`.
- [x] Track worker host, PID, socket path, and runtime directory.
- [x] Clean up stale runtime state.
- [x] Implement `v logs <app>`.

## Phase 4: Volumes

- Implement `v volume create <app>`.
- Manage ext4 data volume files or block devices on the worker.
- Enforce attach policy.
- Enforce single-writer volume locks.
- Attach persistent data volume to the guest.

## Phase 5: Deploy

- Boot a new VM from an immutable image.
- Run health checks.
- Update reverse proxy routing.
- Reload reverse proxy.
- Drain the old VM.
- Stop the old VM.
- Update app registry after a successful switch.

## Phase 6: Rollback

- Store previous image metadata.
- Boot rollback VM.
- Run health checks.
- Switch proxy routing back.
- Stop failed or replaced VM.
- Keep DB schema rollback out of scope for the first implementation.

## Phase 7: Image Build

- Accept existing ext4 rootfs images first.
- Add Docker export based image creation on Mac or dedicated builder.
- Upload or sync built images to the worker.
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
