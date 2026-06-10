# Health Checks

Tamaya checks a process release before switching traffic to it. The app must bind to the `PORT` environment variable; Tamaya checks the release directly on the worker at:

```text
http://127.0.0.1:<allocated-port><path>
```

Defaults:

```toml
[health_check]
path = "/health"
retries = 5
interval_secs = 5
timeout_secs = 2
```

The `[health_check]` table is optional, and each field may be omitted. A check passes when the endpoint returns an HTTP status below `400` before `timeout_secs`. Redirects are not followed, so the endpoint should answer directly.

If the new release does not pass its health check, Tamaya aborts the deploy, cleans up the failed release, and leaves the previous route in place.

Published static and SPA apps do not run health checks.
