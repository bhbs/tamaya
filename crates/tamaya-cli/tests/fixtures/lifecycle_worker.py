#!/usr/bin/env python3
"""Local worker fixture: execute generated scripts; stub only host services."""

import os
from pathlib import Path
import re
import shlex
import sys

root = Path(os.environ["TAMAYA_TEST_WORKER"])
command = Path(sys.argv[0]).name
args = sys.argv[1:]


def fail_once(stage):
    flag = root / ("fail-" + stage)
    if flag.exists():
        flag.unlink()
        print("injected " + stage + " failure", file=sys.stderr)
        sys.exit(1)


def active_ports():
    for service in (root / "active").iterdir():
        unit = (root / "systemd" / service.name).read_text()
        yield re.search(r"^Environment=PORT=(\d+)$", unit, re.M).group(1)


if command == "ssh":
    assert len(args) == 2 and args[0] == "local-fixture", args
    remote = shlex.split(args[1])
    assert len(remote) == 5 and remote[:4] == ["sudo", "-n", "sh", "-lc"], remote
    script = remote[4]
    for original, local in [
        ("/var/lib/tamaya", root / "data"),
        ("/etc/caddy", root / "caddy"),
        ("/etc/systemd/system", root / "systemd"),
        ("/etc/tamaya/apps", root / "env"),
    ]:
        script = script.replace(original, str(local))
        assert original not in script
    # Skip the privilege/login wrapper to retain fixture PATH. Permission tests
    # exercise that wrapper separately with real sudo on Linux.
    os.execv("/bin/sh", ["sh", "-c", script])
elif command == "systemctl":
    with (root / "systemctl.log").open("a") as log:
        log.write(" ".join(args) + "\n")
    if args[0] == "list-unit-files":
        for unit in sorted((root / "systemd").glob(args[1])):
            print(unit.name, "enabled")
    elif args[0] == "enable" and args[1] == "--now":
        assert (root / "systemd" / args[2]).is_file()
        (root / "active" / args[2]).touch()
    elif args[0] == "disable" and args[1] == "--now":
        (root / "active" / args[2]).unlink(missing_ok=True)
    elif args[0] == "is-active" and args[1] == "--quiet":
        sys.exit(0 if (root / "active" / args[2]).is_file() else 3)
    elif args == ["reload", "caddy"]:
        fail_once("reload")
    elif args == ["daemon-reload"] or args[0] == "reset-failed":
        pass
    else:
        raise AssertionError(args)
elif command == "caddy":
    assert args == ["validate", "--config", str(root / "caddy/Caddyfile")], args
    fail_once("validate")
elif command == "ss":
    assert args == ["-H", "-ltn"], args
    # Another process owns the old app port after stop; rollback must allocate
    # a free port instead of relying on the stale stopped metadata.
    for port in {"20000", *active_ports()}:
        print("LISTEN 0 128 127.0.0.1:" + port + " 0.0.0.0:*")
elif command == "curl":
    assert args[:2] == ["-fsS", "--max-time"], args
    match = re.fullmatch(r"http://127\.0\.0\.1:(\d+)/health", args[-1])
    assert match and match.group(1) in set(active_ports()), args
elif command == "ln":
    assert args[0] == "-sfn" and Path(args[-1]).parent == root / "data/apps/web", args
    if Path(args[-1]).name == "previous":
        fail_once("previous-link")
    if Path(args[-1]).name == "current":
        fail_once("current-link")
    os.execv("/bin/ln", ["ln", *args])
elif command == "id":
    if args == ["tamaya-web"]:
        print("uid=1001(tamaya-web) gid=1001(tamaya-web)")
    else:
        os.execv("/usr/bin/id", ["id", *args])
elif command == "cp":
    source = Path(args[-2])
    if source.name == "metadata.toml":
        fail_once("metadata-backup")
    elif source == root / "data/caddy-routes/web.caddy":
        fail_once("route-backup")
    os.execv("/bin/cp", ["cp", *args])
elif command == "mv":
    if Path(args[-1]) == root / "data/apps/web/metadata.toml":
        if Path(args[-2]).name == "metadata.toml.bak":
            fail_once("metadata-restore")
        fail_once("metadata-write")
    os.execv("/bin/mv", ["mv", *args])
elif command == "userdel":
    assert args == ["tamaya-web"], args
else:
    raise AssertionError(command)
