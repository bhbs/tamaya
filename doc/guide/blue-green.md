# Blue-Green Deploy

For process apps, Tamaya keeps the current release serving traffic while the new release starts as a separate systemd unit on a different localhost port. The public Caddy route is changed only after the new release passes its health check. The old systemd unit is stopped after the route switch succeeds.

If upload, startup, health check, Caddy validation/reload, or release-link updates fail, Tamaya restores the original metadata, release links, and route before stopping and removing the new release. A failed first deployment removes the newly created state. If restoration itself fails, Tamaya keeps the candidate release available and reports that manual recovery is needed.

Successful deploys update `current` to the new release and `previous` to the release that was just replaced.

`tamaya rollback` reverses that pair: it starts the previous successful process release on a fresh port, health-checks it, switches traffic back, and stops the formerly current unit.

Published apps are route-only. `tamaya publish` uploads files into an immutable release directory and switches Caddy to that directory. `tamaya rollback` points Caddy back at the previous published release without allocating a port or running a health check.

Publish uses the same failure recovery for metadata, release links, and routes. Stop and delete preserve the previous app state if their Caddy update fails. Stop records the stopped state before shutting down services; delete removes services and data only after the route update succeeds.

Rollback can also resume an app after `tamaya stop`, provided a previous release is available. It records the running release before rebuilding the public route, so stopped apps are included in the restored routing configuration. If the route switch fails, Tamaya restores the original metadata and route; a stopped app stays stopped. Release links change only after the route switch succeeds.
