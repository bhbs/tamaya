const std = @import("std");
const c = @cImport({
    @cInclude("sqlite3.h");
});

pub fn open(database_url: []const u8) !*c.sqlite3 {
    const path = try databasePathFromUrL(database_url);

    std.log.debug("DATABASE_URL={s} dbPath={s}", .{ database_url, path });

    var db: ?*c.sqlite3 = undefined;
    const path_z = try std.posix.toPosixPath(path);
    if (c.sqlite3_open(@ptrCast(&path_z), &db) != c.SQLITE_OK) {
        return error.OpenFailed;
    }

    const db_ptr = db.?;
    _ = c.sqlite3_busy_timeout(db_ptr, 5000);

    _ = c.sqlite3_exec(db_ptr, "PRAGMA journal_mode=WAL", null, null, null);
    _ = c.sqlite3_exec(db_ptr, "PRAGMA foreign_keys=on", null, null, null);

    try migrate(db_ptr);

    return db_ptr;
}

fn databasePathFromUrL(database_url: []const u8) ![]const u8 {
    if (!std.mem.startsWith(u8, database_url, "file:")) {
        return error.InvalidDatabaseUrl;
    }
    return database_url["file:".len..];
}

fn migrate(db: *c.sqlite3) !void {
    const statements = [_][]const u8{
        \\CREATE TABLE IF NOT EXISTS users (
        \\  id TEXT PRIMARY KEY,
        \\  email TEXT NOT NULL UNIQUE,
        \\  password_hash TEXT NOT NULL,
        \\  name TEXT NOT NULL,
        \\  email_verified INTEGER NOT NULL DEFAULT 0,
        \\  created_at TEXT NOT NULL DEFAULT (datetime('now')),
        \\  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        \\)
    ,
        \\CREATE TABLE IF NOT EXISTS sessions (
        \\  id TEXT PRIMARY KEY,
        \\  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        \\  token TEXT NOT NULL UNIQUE,
        \\  expires_at TEXT NOT NULL,
        \\  created_at TEXT NOT NULL DEFAULT (datetime('now'))
        \\)
    ,
        \\CREATE TABLE IF NOT EXISTS verification_tokens (
        \\  id TEXT PRIMARY KEY,
        \\  identifier TEXT NOT NULL,
        \\  token TEXT NOT NULL,
        \\  expires_at TEXT NOT NULL,
        \\  created_at TEXT NOT NULL DEFAULT (datetime('now'))
        \\)
    ,
    };

    for (statements) |stmt| {
        var err_msg: [*c]u8 = undefined;
        if (c.sqlite3_exec(db, stmt.ptr, null, null, &err_msg) != c.SQLITE_OK) {
            std.log.err("migration failed: {s}", .{std.mem.span(err_msg.?)});
            c.sqlite3_free(err_msg);
            return error.MigrateFailed;
        }
    }
}
