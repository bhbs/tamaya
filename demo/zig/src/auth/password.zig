const std = @import("std");

const SALT_LEN = 16;
const HASH_ITERS = 12;
const HASH_LEN = 32;

fn deriveKey(password: []const u8, salt: []const u8) [HASH_LEN]u8 {
    var buf: [128]u8 = undefined;
    @memcpy(buf[0..salt.len], salt);
    @memcpy(buf[salt.len..][0..password.len], password);
    const initial_len = salt.len + password.len;

    var hash = std.crypto.hash.sha2.Sha256.hash(buf[0..initial_len]);

    for (0..(1 << HASH_ITERS) - 1) |_| {
        var combined: [32 + initial_len]u8 = undefined;
        @memcpy(combined[0..32], &hash);
        @memcpy(combined[32..], buf[0..initial_len]);
        hash = std.crypto.hash.sha2.Sha256.hash(&combined);
    }

    return hash;
}

pub fn hashPassword(allocator: std.mem.Allocator, password: []const u8) ![]const u8 {
    var salt: [SALT_LEN]u8 = undefined;
    std.crypto.random.bytes(&salt);

    const hash = deriveKey(password, &salt);

    var hex_salt: [SALT_LEN * 2]u8 = undefined;
    _ = try std.fmt.bufPrint(&hex_salt, "{s}", .{std.fmt.fmtSliceHexLower(&salt)});

    var hex_hash: [HASH_LEN * 2]u8 = undefined;
    _ = try std.fmt.bufPrint(&hex_hash, "{s}", .{std.fmt.fmtSliceHexLower(&hash)});

    return try std.fmt.allocPrint(allocator, "$sha256${d}${s}${s}", .{
        HASH_ITERS,
        &hex_salt,
        &hex_hash,
    });
}

pub fn checkPassword(password: []const u8, stored: []const u8) bool {
    const parts = std.mem.splitSequence(u8, stored, "$");
    _ = parts.next(); // empty before first $
    const scheme = parts.next() orelse return false;
    const iters_str = parts.next() orelse return false;
    const salt_hex = parts.next() orelse return false;
    const hash_hex = parts.next() orelse return false;
    if (parts.next() != null) return false;

    if (!std.mem.eql(u8, scheme, "sha256")) return false;

    const iters = std.fmt.parseInt(u32, iters_str, 10) catch return false;
    _ = iters;

    var salt: [SALT_LEN]u8 = undefined;
    if (salt_hex.len != SALT_LEN * 2) return false;
    _ = std.fmt.hexToBytes(&salt, salt_hex) catch return false;

    var expected_hash: [HASH_LEN]u8 = undefined;
    if (hash_hex.len != HASH_LEN * 2) return false;
    _ = std.fmt.hexToBytes(&expected_hash, hash_hex) catch return false;

    const actual_hash = deriveKey(password, &salt);
    return std.mem.eql(u8, &actual_hash, &expected_hash);
}
