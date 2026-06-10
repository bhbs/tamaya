# CLI Commands

| Command | Description |
|---------|-------------|
| `tamaya setup` | Prepare systemd, Caddy, and worker directories |
| `tamaya check` | Verify worker readiness |
| `tamaya deploy [app]` | Upload and activate a new binary release |
| `tamaya publish [app]` | Upload and activate a static or SPA site |
| `tamaya rollback [app]` | Restore the previous successful release |
| `tamaya status [app]` | Show app release, route, and resource state |
| `tamaya logs [app]` | Stream current release logs from journalctl |
| `tamaya stop [app]` | Stop release units and remove the public route |
| `tamaya delete [app]` | Remove an app; preserve data unless purged |
| `tamaya maintenance [app]` | Replace the public route with HTTP 503 |
| `tamaya live [app]` | Restore the current release route |
| `tamaya version` | Print version information |
| `tamaya env [app] set` | Set app environment variables |
| `tamaya env [app] unset` | Remove app environment variables |
| `tamaya env [app] list` | List app environment variables |

Progress messages are written to stderr so command results on stdout remain pipe-friendly. Interactive terminals show a spinner when an operation takes longer than one second; redirected output uses static progress messages.
