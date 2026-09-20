# Maintenance Mode

Maintenance mode replaces the domain's Caddy route with a static maintenance page. `live` removes the maintenance page and rebuilds the route for the current release.

## Per-App

```bash
tamaya maintenance blog --message "Back shortly"
tamaya live blog
```

`--message` is optional (defaults to "Service temporarily unavailable").
Messages are escaped before being written to the maintenance page.

If Caddy validation or reload fails during `maintenance` or `live`, Tamaya
restores the previous app state, routing, and maintenance page. This also
preserves the previous message when updating a page already in maintenance.
The maintenance page is removed only after Caddy accepts the live routes.
If restoration itself fails, Tamaya keeps the backup and reports its location
for manual recovery instead of rebuilding routes from incomplete state.

`maintenance` and `live` require an app that has not been stopped. `live` changes routing and does not start the app; for process apps, it also checks that the current release's service is active before restoring its route. To resume a stopped app, use `deploy` for a process app, `publish` for a site, or `rollback` to restore the previous release.

## Per-Domain

Put every Tamaya-managed route on a domain into maintenance at once without referencing an app name:

```bash
tamaya maintenance --domain example.com --message "Back shortly"
tamaya live --domain example.com
```

App and domain selectors are mutually exclusive. In app mode, the app can come from `.tamaya.toml`; in domain mode, pass the domain explicitly with `--domain`. The domain must already be known from at least one deployed or published app. Domain maintenance applies to all Tamaya-managed routes on that domain, including path-scoped apps.

Domain maintenance does not change app states. `live --domain` restores routes for apps that have not been stopped; stopped apps remain stopped and unpublished.
