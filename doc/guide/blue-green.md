# Blue-Green Deploy

For process apps, Tamaya keeps the current release serving traffic while the new release starts as a separate systemd unit on a different localhost port. The public Caddy route is changed only after the new release passes its health check. The old systemd unit is stopped after the route switch succeeds.

If upload, startup, health check, or Caddy reload fails, Tamaya stops and removes the new release, restores worker metadata when needed, and leaves the prior route in place.

Successful deploys update `current` to the new release and `previous` to the release that was just replaced.

`tamaya rollback` reverses that pair: it starts the previous successful process release on a fresh port, health-checks it, switches traffic back, and stops the formerly current unit.

Published apps are route-only. `tamaya publish` uploads files into an immutable release directory and switches Caddy to that directory. `tamaya rollback` points Caddy back at the previous published release without allocating a port or running a health check.
