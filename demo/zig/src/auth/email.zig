const std = @import("std");

pub fn initEmail(api_key: []const u8, from_email: []const u8, base_url: []const u8) void {
    _ = api_key;
    _ = from_email;
    _ = base_url;
}

fn getEnv(allocator: std.mem.Allocator, key: []const u8, fallback: []const u8) []const u8 {
    const key_z = allocator.dupeZ(u8, key) catch return fallback;
    defer allocator.free(key_z);
    const value = std.c.getenv(key_z);
    if (value) |v| {
        return allocator.dupe(u8, std.mem.sliceTo(v, 0)) catch fallback;
    }
    return fallback;
}

pub fn sendVerificationEmail(allocator: std.mem.Allocator, email: []const u8, name: []const u8, token: []const u8) void {
    _ = name;
    const app_name = getEnv(allocator, "APP_NAME", "Demo");
    defer allocator.free(app_name);

    const base_url = getEnv(allocator, "BASE_URL", "http://localhost:8080");
    defer allocator.free(base_url);

    const url = std.fmt.allocPrint(allocator, "{s}/api/auth/verify-email?token={s}", .{
        std.mem.trimRight(u8, base_url, "/"),
        token,
    }) catch return;
    defer allocator.free(url);

    const text = std.fmt.allocPrint(allocator,
        \\Use the link below to verify your {s} email address.
        \\
        \\{s}
        \\
        \\If you did not create a {s} account, you can ignore this email.
    , .{ app_name, url, app_name }) catch return;
    defer allocator.free(text);

    const subject = std.fmt.allocPrint(allocator, "Verify your {s} email", .{app_name}) catch return;
    defer allocator.free(subject);

    std.log.info("[EMAIL] To: {s} | Subject: {s} | Body:\n{s}", .{ email, subject, text });
}

pub fn sendPasswordResetEmail(allocator: std.mem.Allocator, email: []const u8, name: []const u8, token: []const u8) void {
    _ = name;
    const app_name = getEnv(allocator, "APP_NAME", "Demo");
    defer allocator.free(app_name);

    const base_url = getEnv(allocator, "BASE_URL", "http://localhost:8080");
    defer allocator.free(base_url);

    const url = std.fmt.allocPrint(allocator, "{s}/reset-password?token={s}", .{
        std.mem.trimRight(u8, base_url, "/"),
        token,
    }) catch return;
    defer allocator.free(url);

    const text = std.fmt.allocPrint(allocator,
        \\Use the link below to reset your {s} password.
        \\
        \\{s}
        \\
        \\If you did not request this, you can ignore this email.
    , .{ app_name, url, app_name }) catch return;
    defer allocator.free(text);

    const subject = std.fmt.allocPrint(allocator, "Reset your {s} password", .{app_name}) catch return;
    defer allocator.free(subject);

    std.log.info("[EMAIL] To: {s} | Subject: {s} | Body:\n{s}", .{ email, subject, text });
}
