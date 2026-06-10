# Maintenance Mode

Maintenance mode replaces the domain's Caddy route with a static maintenance page. `live` removes the maintenance page and rebuilds the route for the current release.

## Per-App

```bash
tamaya maintenance blog --message "Back shortly"
tamaya live blog
```

`--message` is optional (defaults to "Service temporarily unavailable").
Messages are escaped before being written to the maintenance page.

## Per-Domain

Put every Tamaya-managed route on a domain into maintenance at once without referencing an app name:

```bash
tamaya maintenance --domain example.com --message "Back shortly"
tamaya live --domain example.com
```

App and domain selectors are mutually exclusive. In app mode, the app can come from `.tamaya.toml`; in domain mode, pass the domain explicitly with `--domain`. The domain must already be known from at least one deployed or published app. Domain maintenance applies to all Tamaya-managed routes on that domain, including path-scoped apps.
