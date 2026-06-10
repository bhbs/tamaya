const std = @import("std");
const c = @cImport({
    @cInclude("sqlite3.h");
});

const password = @import("password.zig");
const email = @import("email.zig");
const service = @import("service.zig");
const errors = @import("errors.zig");

const Server = std.http.Server;

pub fn dispatch(
    allocator: std.mem.Allocator,
    db: *c.sqlite3,
    res: *Server.Response,
) !void {
    const req = res.request;
    const target = req.target;
    const method = req.method;

    const path = if (std.mem.indexOfScalar(u8, target, '?')) |qp| target[0..qp] else target;

    if (std.mem.eql(u8, path, "/api/auth/signup") and method == .POST) {
        try handleSignUp(allocator, db, res);
    } else if (std.mem.eql(u8, path, "/api/auth/signin") and method == .POST) {
        try handleSignIn(allocator, db, res);
    } else if (std.mem.eql(u8, path, "/api/auth/signout") and method == .POST) {
        try handleSignOut(allocator, db, res);
    } else if (std.mem.eql(u8, path, "/api/auth/session") and method == .GET) {
        try handleSession(allocator, db, res);
    } else if (std.mem.eql(u8, path, "/api/auth/verify-email") and (method == .GET or method == .POST)) {
        try handleVerifyEmail(allocator, db, res);
    } else if (std.mem.eql(u8, path, "/api/auth/forgot-password") and method == .POST) {
        try handleForgotPassword(allocator, db, res);
    } else if (std.mem.eql(u8, path, "/api/auth/reset-password") and method == .POST) {
        try handleResetPassword(allocator, db, res);
    } else if (std.mem.eql(u8, path, "/health") and method == .GET) {
        try handleHealth(res);
    } else {
        res.status = .not_found;
        try respondJson(res, "{\"error\":\"not found\"}");
    }
}

fn respondJson(res: *Server.Response, json: []const u8) !void {
    res.transfer_encoding = .{ .content_length = json.len };
    try res.headers.append("content-type", "application/json");
    try res.do();
    try res.writeAll(json);
    try res.finish();
}

fn respondJsonStatus(res: *Server.Response, status: Server.Response.Status, json: []const u8) !void {
    res.status = status;
    try respondJson(res, json);
}

fn respondJsonWithCookies(res: *Server.Response, status: Server.Response.Status, json: []const u8, cookies: []const u8) !void {
    res.status = status;
    res.transfer_encoding = .{ .content_length = json.len };
    try res.headers.append("content-type", "application/json");
    if (cookies.len > 0) {
        try res.headers.append("set-cookie", cookies);
    }
    try res.do();
    try res.writeAll(json);
    try res.finish();
}

fn getSessionTokenFromRequest(res: *Server.Response) ?[]const u8 {
    const header_value = res.request.headers.get("cookie") orelse return null;
    var iter = std.mem.splitSequence(u8, header_value, ";");
    while (iter.next()) |part| {
        const trimmed = std.mem.trim(u8, part, " ");
        if (std.mem.startsWith(u8, trimmed, "session=")) {
            return trimmed["session=".len..];
        }
    }
    return null;
}

fn readRequestBody(res: *Server.Response, allocator: std.mem.Allocator) ![]u8 {
    var buf = std.ArrayList(u8).init(allocator);
    defer buf.deinit();

    const reader = res.request.reader();
    try reader.readAllArrayList(&buf, 1_000_000);

    return try buf.toOwnedSlice();
}

fn getQueryParam(res: *Server.Response, name: []const u8) ?[]const u8 {
    const target = res.request.target;
    const qm = std.mem.indexOfScalar(u8, target, '?') orelse return null;
    const query = target[qm + 1 ..];

    var iter = std.mem.splitSequence(u8, query, "&");
    while (iter.next()) |pair| {
        if (std.mem.indexOfScalar(u8, pair, '=')) |eq| {
            const key = pair[0..eq];
            if (std.mem.eql(u8, key, name)) {
                const value = pair[eq + 1 ..];
                if (value.len == 0) return "";
                return value;
            }
        }
    }
    return null;
}

fn handleHealth(res: *Server.Response) !void {
    try respondJson(res, "{\"status\":\"ok\"}");
}

fn handleSignUp(allocator: std.mem.Allocator, db: *c.sqlite3, res: *Server.Response) !void {
    const body = try readRequestBody(res, allocator);
    defer allocator.free(body);

    const parsed = std.json.parseFromSlice(
        std.json.Value,
        allocator,
        body,
        .{},
    ) catch {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"invalid json\"}");
    };
    defer parsed.deinit();

    const obj = parsed.value.object;
    const email_str = obj.get("email") orelse {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"email is required\"}");
    };
    const password_str = obj.get("password") orelse {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"password is required\"}");
    };

    if (email_str != .string or email_str.string.len == 0) {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"email is required\"}");
    }
    if (password_str != .string or password_str.string.len == 0) {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"password is required\"}");
    }

    const user = service.createUser(allocator, db, email_str.string, password_str.string) catch |err| {
        switch (err) {
            errors.AuthError.EmailTaken => return respondJsonStatus(res, .@"conflict", "{\"error\":\"could not create that account\"}"),
            else => {
                std.log.err("signup: {s}", .{@errorName(err)});
                return respondJsonStatus(res, .internal_server_error, "{\"error\":\"could not create that account\"}");
            },
        }
    };

    service.createVerificationToken(allocator, db, user.email) catch {};
    email.sendVerificationEmail(allocator, user.email, user.name, "");

    try respondJsonStatus(res, .created, "{\"message\":\"Check your email to verify your account before signing in.\"}");
}

fn handleSignIn(allocator: std.mem.Allocator, db: *c.sqlite3, res: *Server.Response) !void {
    const body = try readRequestBody(res, allocator);
    defer allocator.free(body);

    const parsed = std.json.parseFromSlice(
        std.json.Value,
        allocator,
        body,
        .{},
    ) catch {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"invalid json\"}");
    };
    defer parsed.deinit();

    const obj = parsed.value.object;
    const email_str = obj.get("email") orelse {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"email is required\"}");
    };
    const password_str = obj.get("password") orelse {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"password is required\"}");
    };

    if (email_str != .string or email_str.string.len == 0) {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"email is required\"}");
    }
    if (password_str != .string or password_str.string.len == 0) {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"password is required\"}");
    }

    const result = service.getUserByEmail(allocator, db, email_str.string) catch {
        return respondJsonStatus(res, .unauthorized, "{\"error\":\"invalid email or password\"}");
    };

    if (!password.checkPassword(password_str.string, result.password_hash)) {
        defer allocator.free(result.password_hash);
        return respondJsonStatus(res, .unauthorized, "{\"error\":\"invalid email or password\"}");
    }
    defer allocator.free(result.password_hash);

    if (!result.user.email_verified) {
        const vt = service.createVerificationToken(allocator, db, result.user.email) catch null;
        if (vt) |token| {
            defer allocator.free(token);
            email.sendVerificationEmail(allocator, result.user.email, result.user.name, token);
        }
        return respondJsonStatus(res, .forbidden, "{\"error\":\"please verify your email address before signing in. we sent a new verification link\"}");
    }

    const session = service.createSession(allocator, db, result.user.id) catch {
        return respondJsonStatus(res, .internal_server_error, "{\"error\":\"could not sign in\"}");
    };

    const cookie = std.fmt.allocPrint(allocator, "session={s}; Path=/; HttpOnly; SameSite=Lax; Max-Age={d}", .{
        session.token,
        30 * 24 * 60 * 60,
    }) catch {
        return respondJsonStatus(res, .internal_server_error, "{\"error\":\"could not sign in\"}");
    };
    defer allocator.free(cookie);

    const user_json = std.fmt.allocPrint(allocator,
        \\{{"user":{{"id":"{s}","email":"{s}","name":"{s}","emailVerified":{s},"createdAt":"{s}"}},"redirectTo":"/"}}
    , .{
        result.user.id,
        result.user.email,
        result.user.name,
        if (result.user.email_verified) "true" else "false",
        result.user.created_at,
    }) catch {
        return respondJsonStatus(res, .internal_server_error, "{\"error\":\"could not sign in\"}");
    };
    defer allocator.free(user_json);

    try respondJsonWithCookies(res, .ok, user_json, cookie);
}

fn handleSignOut(allocator: std.mem.Allocator, db: *c.sqlite3, res: *Server.Response) !void {
    if (getSessionTokenFromRequest(res)) |token| {
        service.deleteSession(db, token) catch {};
    }

    const clear_cookie = "session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=-1";
    _ = allocator;

    try respondJsonWithCookies(res, .ok, "{\"message\":\"signed out\"}", clear_cookie);
}

fn handleSession(allocator: std.mem.Allocator, db: *c.sqlite3, res: *Server.Response) !void {
    const token = getSessionTokenFromRequest(res) orelse {
        return respondJson(res, "{\"user\":null}");
    };

    const result = service.getSession(allocator, db, token) catch null;
    if (result) |r| {
        const user_json = std.fmt.allocPrint(allocator,
            \\{{"user":{{"id":"{s}","email":"{s}","name":"{s}","emailVerified":{s},"createdAt":"{s}"}}}}
        , .{
            r.user.id,
            r.user.email,
            r.user.name,
            if (r.user.email_verified) "true" else "false",
            r.user.created_at,
        }) catch {
            return respondJson(res, "{\"user\":null}");
        };
        defer allocator.free(user_json);
        return respondJson(res, user_json);
    }

    return respondJson(res, "{\"user\":null}");
}

fn handleVerifyEmail(allocator: std.mem.Allocator, db: *c.sqlite3, res: *Server.Response) !void {
    var token_val: ?[]const u8 = getQueryParam(res, "token");

    if (token_val == null and res.request.method == .POST) {
        const body = try readRequestBody(res, allocator);
        defer allocator.free(body);

        const parsed = std.json.parseFromSlice(
            std.json.Value,
            allocator,
            body,
            .{},
        ) catch null;
        if (parsed) |p| {
            defer p.deinit();
            if (p.value.object.get("token")) |t| {
                if (t == .string and t.string.len > 0) {
                    token_val = t.string;
                }
            }
        }
    }

    const token = token_val orelse {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"token is required\"}");
    };

    const redirect_url = std.fmt.allocPrint(allocator, "/signin?verified=1", .{}) catch {
        return respondJsonStatus(res, .internal_server_error, "{\"error\":\"internal error\"}");
    };
    defer allocator.free(redirect_url);

    service.verifyEmail(db, token) catch |err| {
        switch (err) {
            errors.AuthError.InvalidToken, errors.AuthError.TokenExpired => {
                return respondRedirect(res, "/signin?error=This+verification+link+is+invalid+or+expired");
            },
            else => {
                std.log.err("verify email: {s}", .{@errorName(err)});
                return respondRedirect(res, "/signin?error=Could+not+verify+email");
            },
        }
    };

    return respondRedirect(res, redirect_url);
}

fn handleForgotPassword(allocator: std.mem.Allocator, db: *c.sqlite3, res: *Server.Response) !void {
    const body = try readRequestBody(res, allocator);
    defer allocator.free(body);

    const parsed = std.json.parseFromSlice(
        std.json.Value,
        allocator,
        body,
        .{},
    ) catch {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"invalid json\"}");
    };
    defer parsed.deinit();

    const obj = parsed.value.object;
    const email_str = obj.get("email") orelse {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"email is required\"}");
    };

    if (email_str != .string or email_str.string.len == 0) {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"email is required\"}");
    }

    const message = "{\"message\":\"if that email exists, a password reset link has been sent\"}";
    const exists = service.userExists(db, email_str.string) catch {
        return respondJson(res, message);
    };

    if (!exists) {
        return respondJson(res, message);
    }

    const vt_token = service.createVerificationToken(allocator, db, email_str.string) catch null;
    if (vt_token) |token| {
        defer allocator.free(token);
        email.sendPasswordResetEmail(allocator, email_str.string, email_str.string, token);
    }

    return respondJson(res, message);
}

fn handleResetPassword(allocator: std.mem.Allocator, db: *c.sqlite3, res: *Server.Response) !void {
    const body = try readRequestBody(res, allocator);
    defer allocator.free(body);

    const parsed = std.json.parseFromSlice(
        std.json.Value,
        allocator,
        body,
        .{},
    ) catch {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"invalid json\"}");
    };
    defer parsed.deinit();

    const obj = parsed.value.object;

    const token_val = obj.get("token") orelse {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"reset token is missing\"}");
    };
    if (token_val != .string or token_val.string.len == 0) {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"reset token is missing\"}");
    }

    const password_val = obj.get("password") orelse {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"password must be at least 8 characters\"}");
    };
    if (password_val != .string or password_val.string.len < 8) {
        return respondJsonStatus(res, .bad_request, "{\"error\":\"password must be at least 8 characters\"}");
    }

    service.resetPassword(allocator, db, token_val.string, password_val.string) catch |err| {
        switch (err) {
            errors.AuthError.InvalidToken, errors.AuthError.TokenExpired => {
                return respondJsonStatus(res, .bad_request, "{\"error\":\"that reset link is invalid or expired\"}");
            },
            else => {
                std.log.err("reset password: {s}", .{@errorName(err)});
                return respondJsonStatus(res, .internal_server_error, "{\"error\":\"could not reset password\"}");
            },
        }
    };

    return respondJson(res, "{\"message\":\"your password has been reset\"}");
}

fn respondRedirect(res: *Server.Response, location: []const u8) !void {
    res.status = .temporary_redirect;
    res.transfer_encoding = .{ .content_length = 0 };
    try res.headers.append("location", location);
    try res.do();
    try res.finish();
}
