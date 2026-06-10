const std = @import("std");

pub const Config = struct {
    database_url: []const u8,
    base_url: []const u8,
    port: []const u8,
    session_secret: []const u8,
};

pub fn load(allocator: std.mem.Allocator) !Config {
    const default_database_url = try defaultDatabaseUrl(allocator);
    defer allocator.free(default_database_url);

    return Config{
        .database_url = try getEnv(allocator, "DATABASE_URL", default_database_url),
        .base_url = try getEnv(allocator, "BASE_URL", "http://localhost:8080"),
        .port = try getEnv(allocator, "PORT", "8080"),
        .session_secret = try getEnv(allocator, "SESSION_SECRET", "change-me-in-production"),
    };
}

fn defaultDatabaseUrl(allocator: std.mem.Allocator) ![]const u8 {
    const data_dir = std.c.getenv("TAMAYA_DATA_DIR") orelse return allocator.dupe(u8, "file:./demo.db");
    return std.fmt.allocPrint(allocator, "file:{s}/demo.db", .{std.mem.sliceTo(data_dir, 0)});
}

fn getEnv(allocator: std.mem.Allocator, key: []const u8, fallback: []const u8) ![]const u8 {
    const key_z = try allocator.dupeZ(u8, key);
    defer allocator.free(key_z);

    const value = std.c.getenv(key_z);
    if (value) |v| {
        return allocator.dupe(u8, std.mem.sliceTo(v, 0));
    }
    return allocator.dupe(u8, fallback);
}
