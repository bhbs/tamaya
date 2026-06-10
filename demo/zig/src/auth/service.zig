const std = @import("std");
const c = @cImport({
    @cInclude("sqlite3.h");
});

const password = @import("password.zig");
const errors = @import("errors.zig");

pub const User = struct {
    id: []const u8,
    email: []const u8,
    name: []const u8,
    email_verified: bool,
    created_at: []const u8,
};

pub const Session = struct {
    id: []const u8,
    user_id: []const u8,
    token: []const u8,
    expires_at: []const u8,
    created_at: []const u8,
};

pub fn createUser(
    allocator: std.mem.Allocator,
    db: *c.sqlite3,
    email: []const u8,
    plain_password: []const u8,
) !User {
    const hash = try password.hashPassword(allocator, plain_password);
    defer allocator.free(hash);

    const user_id = try generateUuidV4(allocator);
    defer allocator.free(user_id);

    const now = try utcNow(allocator);
    defer allocator.free(now);

    var stmt: ?*c.sqlite3_stmt = undefined;
    defer _ = c.sqlite3_finalize(stmt);

    const sql =
        \\INSERT INTO users (id, email, password_hash, name, email_verified, created_at, updated_at)
        \\VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)
    ;
    if (c.sqlite3_prepare_v2(db, sql.ptr, @intCast(sql.len), &stmt, null) != c.SQLITE_OK) {
        return error.Unexpected;
    }

    _ = c.sqlite3_bind_text(stmt, 1, user_id.ptr, @intCast(user_id.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_bind_text(stmt, 2, email.ptr, @intCast(email.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_bind_text(stmt, 3, hash.ptr, @intCast(hash.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_bind_text(stmt, 4, email.ptr, @intCast(email.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_bind_text(stmt, 5, now.ptr, @intCast(now.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_bind_text(stmt, 6, now.ptr, @intCast(now.len), c.SQLITE_TRANSIENT);

    const rc = c.sqlite3_step(stmt);
    if (rc != c.SQLITE_DONE) {
        if (rc == c.SQLITE_CONSTRAINT) {
            return errors.AuthError.EmailTaken;
        }
        return error.Unexpected;
    }

    return User{
        .id = try allocator.dupe(u8, user_id),
        .email = try allocator.dupe(u8, email),
        .name = try allocator.dupe(u8, email),
        .email_verified = false,
        .created_at = try allocator.dupe(u8, now),
    };
}

pub fn getUserByEmail(
    allocator: std.mem.Allocator,
    db: *c.sqlite3,
    email: []const u8,
) !struct { user: User, password_hash: []const u8 } {
    var stmt: ?*c.sqlite3_stmt = undefined;
    defer _ = c.sqlite3_finalize(stmt);

    const sql =
        \\SELECT id, email, name, email_verified, password_hash, created_at
        \\FROM users WHERE email = ?1
    ;
    if (c.sqlite3_prepare_v2(db, sql.ptr, @intCast(sql.len), &stmt, null) != c.SQLITE_OK) {
        return error.Unexpected;
    }

    _ = c.sqlite3_bind_text(stmt, 1, email.ptr, @intCast(email.len), c.SQLITE_TRANSIENT);

    const rc = c.sqlite3_step(stmt);
    if (rc != c.SQLITE_ROW) {
        return errors.AuthError.InvalidCredentials;
    }

    const id = readColumnText(stmt, 0);
    const em = readColumnText(stmt, 1);
    const name = readColumnText(stmt, 2);
    const email_verified = c.sqlite3_column_int(stmt, 3) != 0;
    const pass_hash = readColumnText(stmt, 4);
    const created_at = readColumnText(stmt, 5);

    return .{
        .user = User{
            .id = try allocator.dupe(u8, std.mem.span(id)),
            .email = try allocator.dupe(u8, std.mem.span(em)),
            .name = try allocator.dupe(u8, std.mem.span(name)),
            .email_verified = email_verified,
            .created_at = try allocator.dupe(u8, std.mem.span(created_at)),
        },
        .password_hash = try allocator.dupe(u8, std.mem.span(pass_hash)),
    };
}

pub fn getUserById(
    allocator: std.mem.Allocator,
    db: *c.sqlite3,
    id: []const u8,
) !?User {
    var stmt: ?*c.sqlite3_stmt = undefined;
    defer _ = c.sqlite3_finalize(stmt);

    const sql =
        \\SELECT id, email, name, email_verified, created_at
        \\FROM users WHERE id = ?1
    ;
    if (c.sqlite3_prepare_v2(db, sql.ptr, @intCast(sql.len), &stmt, null) != c.SQLITE_OK) {
        return error.Unexpected;
    }

    _ = c.sqlite3_bind_text(stmt, 1, id.ptr, @intCast(id.len), c.SQLITE_TRANSIENT);

    const rc = c.sqlite3_step(stmt);
    if (rc != c.SQLITE_ROW) {
        return null;
    }

    return User{
        .id = try allocator.dupe(u8, std.mem.span(readColumnText(stmt, 0))),
        .email = try allocator.dupe(u8, std.mem.span(readColumnText(stmt, 1))),
        .name = try allocator.dupe(u8, std.mem.span(readColumnText(stmt, 2))),
        .email_verified = c.sqlite3_column_int(stmt, 3) != 0,
        .created_at = try allocator.dupe(u8, std.mem.span(readColumnText(stmt, 4))),
    };
}

pub fn createSession(
    allocator: std.mem.Allocator,
    db: *c.sqlite3,
    user_id: []const u8,
) !Session {
    const session_id = try generateUuidV4(allocator);
    defer allocator.free(session_id);

    const token = try generateUuidV4(allocator);
    defer allocator.free(token);

    const now = try utcNow(allocator);
    defer allocator.free(now);

    const expires_at = try utcInDays(allocator, 30);
    defer allocator.free(expires_at);

    var stmt: ?*c.sqlite3_stmt = undefined;
    defer _ = c.sqlite3_finalize(stmt);

    const sql =
        \\INSERT INTO sessions (id, user_id, token, expires_at, created_at)
        \\VALUES (?1, ?2, ?3, ?4, ?5)
    ;
    if (c.sqlite3_prepare_v2(db, sql.ptr, @intCast(sql.len), &stmt, null) != c.SQLITE_OK) {
        return error.Unexpected;
    }

    _ = c.sqlite3_bind_text(stmt, 1, session_id.ptr, @intCast(session_id.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_bind_text(stmt, 2, user_id.ptr, @intCast(user_id.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_bind_text(stmt, 3, token.ptr, @intCast(token.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_bind_text(stmt, 4, expires_at.ptr, @intCast(expires_at.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_bind_text(stmt, 5, now.ptr, @intCast(now.len), c.SQLITE_TRANSIENT);

    if (c.sqlite3_step(stmt) != c.SQLITE_DONE) {
        return error.Unexpected;
    }

    return Session{
        .id = try allocator.dupe(u8, session_id),
        .user_id = try allocator.dupe(u8, user_id),
        .token = try allocator.dupe(u8, token),
        .expires_at = try allocator.dupe(u8, expires_at),
        .created_at = try allocator.dupe(u8, now),
    };
}

pub fn getSession(
    allocator: std.mem.Allocator,
    db: *c.sqlite3,
    token: []const u8,
) !?struct { session: Session, user: User } {
    var stmt: ?*c.sqlite3_stmt = undefined;
    defer _ = c.sqlite3_finalize(stmt);

    const sql =
        \\SELECT id, user_id, token, expires_at, created_at
        \\FROM sessions WHERE token = ?1
    ;
    if (c.sqlite3_prepare_v2(db, sql.ptr, @intCast(sql.len), &stmt, null) != c.SQLITE_OK) {
        return error.Unexpected;
    }

    _ = c.sqlite3_bind_text(stmt, 1, token.ptr, @intCast(token.len), c.SQLITE_TRANSIENT);

    const rc = c.sqlite3_step(stmt);
    if (rc != c.SQLITE_ROW) {
        return null;
    }

    const session = Session{
        .id = try allocator.dupe(u8, std.mem.span(readColumnText(stmt, 0))),
        .user_id = try allocator.dupe(u8, std.mem.span(readColumnText(stmt, 1))),
        .token = try allocator.dupe(u8, std.mem.span(readColumnText(stmt, 2))),
        .expires_at = try allocator.dupe(u8, std.mem.span(readColumnText(stmt, 3))),
        .created_at = try allocator.dupe(u8, std.mem.span(readColumnText(stmt, 4))),
    };

    if (try getUserById(allocator, db, session.user_id)) |user| {
        return .{ .session = session, .user = user };
    }

    return null;
}

pub fn deleteSession(db: *c.sqlite3, token: []const u8) !void {
    var stmt: ?*c.sqlite3_stmt = undefined;
    defer _ = c.sqlite3_finalize(stmt);

    const sql = "DELETE FROM sessions WHERE token = ?1";
    if (c.sqlite3_prepare_v2(db, sql.ptr, @intCast(sql.len), &stmt, null) != c.SQLITE_OK) {
        return error.Unexpected;
    }

    _ = c.sqlite3_bind_text(stmt, 1, token.ptr, @intCast(token.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_step(stmt);
}

pub fn createVerificationToken(
    allocator: std.mem.Allocator,
    db: *c.sqlite3,
    identifier: []const u8,
) ![]const u8 {
    const vt_id = try generateUuidV4(allocator);
    defer allocator.free(vt_id);

    const token = try generateUuidV4(allocator);
    defer allocator.free(token);

    const now = try utcNow(allocator);
    defer allocator.free(now);

    const expires_at = try utcInHours(allocator, 24);
    defer allocator.free(expires_at);

    var stmt: ?*c.sqlite3_stmt = undefined;
    defer _ = c.sqlite3_finalize(stmt);

    const sql =
        \\INSERT INTO verification_tokens (id, identifier, token, expires_at, created_at)
        \\VALUES (?1, ?2, ?3, ?4, ?5)
    ;
    if (c.sqlite3_prepare_v2(db, sql.ptr, @intCast(sql.len), &stmt, null) != c.SQLITE_OK) {
        return error.Unexpected;
    }

    _ = c.sqlite3_bind_text(stmt, 1, vt_id.ptr, @intCast(vt_id.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_bind_text(stmt, 2, identifier.ptr, @intCast(identifier.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_bind_text(stmt, 3, token.ptr, @intCast(token.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_bind_text(stmt, 4, expires_at.ptr, @intCast(expires_at.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_bind_text(stmt, 5, now.ptr, @intCast(now.len), c.SQLITE_TRANSIENT);

    if (c.sqlite3_step(stmt) != c.SQLITE_DONE) {
        return error.Unexpected;
    }

    return try allocator.dupe(u8, token);
}

pub fn verifyEmail(db: *c.sqlite3, token: []const u8) !void {
    var stmt: ?*c.sqlite3_stmt = undefined;
    defer _ = c.sqlite3_finalize(stmt);

    const sql =
        \\SELECT id, identifier, expires_at FROM verification_tokens WHERE token = ?1
    ;
    if (c.sqlite3_prepare_v2(db, sql.ptr, @intCast(sql.len), &stmt, null) != c.SQLITE_OK) {
        return error.Unexpected;
    }

    _ = c.sqlite3_bind_text(stmt, 1, token.ptr, @intCast(token.len), c.SQLITE_TRANSIENT);

    if (c.sqlite3_step(stmt) != c.SQLITE_ROW) {
        return errors.AuthError.InvalidToken;
    }

    const vt_id = std.mem.span(readColumnText(stmt, 0));
    const identifier = std.mem.span(readColumnText(stmt, 1));
    _ = std.mem.span(readColumnText(stmt, 2));

    c.sqlite3_finalize(stmt);
    stmt = null;

    var update_stmt: ?*c.sqlite3_stmt = undefined;
    defer _ = c.sqlite3_finalize(update_stmt);

    const update_sql = "UPDATE users SET email_verified = 1 WHERE email = ?1";
    if (c.sqlite3_prepare_v2(db, update_sql.ptr, @intCast(update_sql.len), &update_stmt, null) != c.SQLITE_OK) {
        return error.Unexpected;
    }

    _ = c.sqlite3_bind_text(update_stmt, 1, identifier.ptr, @intCast(identifier.len), c.SQLITE_TRANSIENT);

    if (c.sqlite3_step(update_stmt) != c.SQLITE_DONE) {
        return error.Unexpected;
    }

    c.sqlite3_finalize(update_stmt);

    var delete_stmt: ?*c.sqlite3_stmt = undefined;
    defer _ = c.sqlite3_finalize(delete_stmt);

    const delete_sql = "DELETE FROM verification_tokens WHERE id = ?1";
    if (c.sqlite3_prepare_v2(db, delete_sql.ptr, @intCast(delete_sql.len), &delete_stmt, null) != c.SQLITE_OK) {
        return error.Unexpected;
    }

    _ = c.sqlite3_bind_text(delete_stmt, 1, vt_id.ptr, @intCast(vt_id.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_step(delete_stmt);
}

pub fn resetPassword(
    allocator: std.mem.Allocator,
    db: *c.sqlite3,
    token: []const u8,
    new_password: []const u8,
) !void {
    var stmt: ?*c.sqlite3_stmt = undefined;
    defer _ = c.sqlite3_finalize(stmt);

    const sql =
        \\SELECT id, identifier FROM verification_tokens WHERE token = ?1
    ;
    if (c.sqlite3_prepare_v2(db, sql.ptr, @intCast(sql.len), &stmt, null) != c.SQLITE_OK) {
        return error.Unexpected;
    }

    _ = c.sqlite3_bind_text(stmt, 1, token.ptr, @intCast(token.len), c.SQLITE_TRANSIENT);

    if (c.sqlite3_step(stmt) != c.SQLITE_ROW) {
        return errors.AuthError.InvalidToken;
    }

    const vt_id = std.mem.span(readColumnText(stmt, 0));
    const identifier = std.mem.span(readColumnText(stmt, 1));

    c.sqlite3_finalize(stmt);
    stmt = null;

    const hash = try password.hashPassword(allocator, new_password);
    defer allocator.free(hash);

    const now = try utcNow(allocator);
    defer allocator.free(now);

    var update_stmt: ?*c.sqlite3_stmt = undefined;
    defer _ = c.sqlite3_finalize(update_stmt);

    const update_sql =
        \\UPDATE users SET password_hash = ?1, updated_at = ?2 WHERE email = ?3
    ;
    if (c.sqlite3_prepare_v2(db, update_sql.ptr, @intCast(update_sql.len), &update_stmt, null) != c.SQLITE_OK) {
        return error.Unexpected;
    }

    _ = c.sqlite3_bind_text(update_stmt, 1, hash.ptr, @intCast(hash.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_bind_text(update_stmt, 2, now.ptr, @intCast(now.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_bind_text(update_stmt, 3, identifier.ptr, @intCast(identifier.len), c.SQLITE_TRANSIENT);

    if (c.sqlite3_step(update_stmt) != c.SQLITE_DONE) {
        return error.Unexpected;
    }

    c.sqlite3_finalize(update_stmt);

    var delete_stmt: ?*c.sqlite3_stmt = undefined;
    defer _ = c.sqlite3_finalize(delete_stmt);

    const delete_sql = "DELETE FROM verification_tokens WHERE id = ?1";
    if (c.sqlite3_prepare_v2(db, delete_sql.ptr, @intCast(delete_sql.len), &delete_stmt, null) != c.SQLITE_OK) {
        return error.Unexpected;
    }

    _ = c.sqlite3_bind_text(delete_stmt, 1, vt_id.ptr, @intCast(vt_id.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_step(delete_stmt);

    c.sqlite3_finalize(delete_stmt);

    var revoke_stmt: ?*c.sqlite3_stmt = undefined;
    defer _ = c.sqlite3_finalize(revoke_stmt);

    const revoke_sql =
        \\DELETE FROM sessions WHERE user_id IN (SELECT id FROM users WHERE email = ?1)
    ;
    if (c.sqlite3_prepare_v2(db, revoke_sql.ptr, @intCast(revoke_sql.len), &revoke_stmt, null) != c.SQLITE_OK) {
        return error.Unexpected;
    }

    _ = c.sqlite3_bind_text(revoke_stmt, 1, identifier.ptr, @intCast(identifier.len), c.SQLITE_TRANSIENT);
    _ = c.sqlite3_step(revoke_stmt);
}

pub fn userExists(db: *c.sqlite3, email: []const u8) !bool {
    var stmt: ?*c.sqlite3_stmt = undefined;
    defer _ = c.sqlite3_finalize(stmt);

    const sql = "SELECT EXISTS(SELECT 1 FROM users WHERE email = ?1)";
    if (c.sqlite3_prepare_v2(db, sql.ptr, @intCast(sql.len), &stmt, null) != c.SQLITE_OK) {
        return error.Unexpected;
    }

    _ = c.sqlite3_bind_text(stmt, 1, email.ptr, @intCast(email.len), c.SQLITE_TRANSIENT);

    if (c.sqlite3_step(stmt) != c.SQLITE_ROW) {
        return error.Unexpected;
    }

    return c.sqlite3_column_int(stmt, 0) != 0;
}

fn readColumnText(stmt: ?*c.sqlite3_stmt, col: c_int) [*c]const u8 {
    return @constCast(@ptrCast(c.sqlite3_column_text(stmt, col)));
}

fn generateUuidV4(allocator: std.mem.Allocator) ![]const u8 {
    var bytes: [16]u8 = undefined;
    std.crypto.random.bytes(&bytes);

    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    return try std.fmt.allocPrint(allocator,
        "{x:0>2}{x:0>2}{x:0>2}{x:0>2}-{x:0>2}{x:0>2}-{x:0>2}{x:0>2}-{x:0>2}{x:0>2}-{x:0>2}{x:0>2}{x:0>2}{x:0>2}{x:0>2}{x:0>2}",
        .{
            bytes[0],  bytes[1],  bytes[2],  bytes[3],
            bytes[4],  bytes[5],
            bytes[6],  bytes[7],
            bytes[8],  bytes[9],
            bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        },
    );
}

fn utcNow(allocator: std.mem.Allocator) ![]const u8 {
    const now = std.time.timestamp();
    const epoch_seconds: u64 = @intCast(@abs(now));
    const seconds_per_day: u64 = 86400;
    const unix_epoch_days: u64 = 719468;

    const days_since_unix = epoch_seconds / seconds_per_day;
    const time_of_day = epoch_seconds % seconds_per_day;
    const days = days_since_unix + unix_epoch_days;

    const quadcent: u64 = days / 146097;
    const dqc: u64 = days % 146097;
    const cent: u64 = @min(dqc / 36524, 3);
    const dcent: u64 = dqc - cent * 36524;
    const quad: u64 = dcent / 1461;
    const dquad: u64 = dcent % 1461;
    const yindex: u64 = @min(dquad / 365, 3);

    var year: u64 = quadcent * 400 + cent * 100 + quad * 4 + yindex;
    if (cent != 3 and yindex != 3) {
        year += 1;
    }

    const is_leap = (year % 4 == 0) and ((year % 100 != 0) or (year % 400 == 0));
    const days_in_feb = if (is_leap) @as(u64, 29) else 28;
    const days_in_month = [_]u64{ 31, days_in_feb, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31 };

    var doy = dquad - (yindex * 365);
    if (cent == 3 or yindex == 3) {
        doy += if (is_leap) @as(u64, 366) else 365;
    }

    var month: u64 = 1;
    var remaining = doy;
    for (days_in_month) |dim| {
        if (remaining < dim) break;
        remaining -= dim;
        month += 1;
    }
    const day = remaining + 1;

    const hours = (time_of_day / 3600) % 24;
    const minutes = (time_of_day % 3600) / 60;
    const seconds = time_of_day % 60;

    return try std.fmt.allocPrint(allocator, "{d:0>4}-{d:0>2}-{d:0>2}T{d:0>2}:{d:0>2}:{d:0>2}Z", .{
        year, month, day, hours, minutes, seconds,
    });
}

fn utcInDays(allocator: std.mem.Allocator, days: u32) ![]const u8 {
    const now = std.time.timestamp();
    const future = now + @as(i64, days) * 86400;
    return timestampToRfc3339(allocator, future);
}

fn utcInHours(allocator: std.mem.Allocator, hours: u32) ![]const u8 {
    const now = std.time.timestamp();
    const future = now + @as(i64, hours) * 3600;
    return timestampToRfc3339(allocator, future);
}

fn timestampToRfc3339(allocator: std.mem.Allocator, ts: i64) ![]const u8 {
    const epoch_seconds: u64 = @intCast(@abs(ts));
    const seconds_per_day: u64 = 86400;
    const unix_epoch_days: u64 = 719468;

    const days_since_unix = epoch_seconds / seconds_per_day;
    const time_of_day = epoch_seconds % seconds_per_day;
    const days = days_since_unix + unix_epoch_days;

    const quadcent: u64 = days / 146097;
    const dqc: u64 = days % 146097;
    const cent: u64 = @min(dqc / 36524, 3);
    const dcent: u64 = dqc - cent * 36524;
    const quad: u64 = dcent / 1461;
    const dquad: u64 = dcent % 1461;
    const yindex: u64 = @min(dquad / 365, 3);

    var year: u64 = quadcent * 400 + cent * 100 + quad * 4 + yindex;
    if (cent != 3 and yindex != 3) {
        year += 1;
    }

    const is_leap = (year % 4 == 0) and ((year % 100 != 0) or (year % 400 == 0));
    const days_in_feb = if (is_leap) @as(u64, 29) else 28;
    const days_in_month = [_]u64{ 31, days_in_feb, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31 };

    var doy = dquad - (yindex * 365);
    if (cent == 3 or yindex == 3) {
        doy += if (is_leap) @as(u64, 366) else 365;
    }

    var month: u64 = 1;
    var remaining = doy;
    for (days_in_month) |dim| {
        if (remaining < dim) break;
        remaining -= dim;
        month += 1;
    }
    const day = remaining + 1;

    const hours = (time_of_day / 3600) % 24;
    const minutes = (time_of_day % 3600) / 60;
    const seconds = time_of_day % 60;

    return try std.fmt.allocPrint(allocator, "{d:0>4}-{d:0>2}-{d:0>2}T{d:0>2}:{d:0>2}:{d:0>2}Z", .{
        year, month, day, hours, minutes, seconds,
    });
}
