const std = @import("std");
const posix = std.posix;
const cImport = @cImport({
    @cInclude("sqlite3.h");
});

const config = @import("config.zig");
const db = @import("db.zig");

const index_html = @embedFile("static/index.html");
const js_bundle = @embedFile("static/assets/index-KaVnzfy_.js");
const css_bundle = @embedFile("static/assets/index-OWTbmwCm.css");

pub fn main() !void {
    var gpa = std.heap.DebugAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const cfg = try config.load(allocator);
    defer {
        allocator.free(cfg.database_url);
        allocator.free(cfg.base_url);
        allocator.free(cfg.port);
        allocator.free(cfg.session_secret);
    }

    const database = try db.open(cfg.database_url);
    defer _ = cImport.sqlite3_close(@ptrCast(database));

    const port: u16 = try std.fmt.parseInt(u16, cfg.port, 10);

    var threaded = std.Io.Threaded.init(allocator, .{});
    defer threaded.deinit();
    const io = threaded.io();

    const addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", port);
    var server = try addr.listen(io, .{ .reuse_address = true });
    defer server.deinit(io);

    std.log.info("Demo server starting on :{d} (embedded static)", .{port});

    var app = App{ .allocator = allocator, .db = @ptrCast(database) };

    while (true) {
        var stream = server.accept(io) catch |err| {
            std.log.err("accept: {s}", .{@errorName(err)});
            continue;
        };
        defer stream.close(io);

        const fd: posix.fd_t = @intCast(stream.socket.handle);
        handleConnection(&app, fd) catch |err| {
            if (err != error.EndOfStream) {
                std.log.err("handle: {s}", .{@errorName(err)});
            }
        };
    }
}

const App = struct {
    allocator: std.mem.Allocator,
    db: *anyopaque,
};

fn handleConnection(app: *const App, fd: posix.fd_t) !void {
    var req_buf: [4096]u8 = undefined;
    const n = try posix.read(fd, &req_buf);
    if (n == 0) return;

    const request = req_buf[0..n];

    var line_iter = std.mem.splitSequence(u8, request, "\r\n");
    const first_line = line_iter.next() orelse return;

    var parts = std.mem.splitSequence(u8, first_line, " ");
    const method_str = parts.next() orelse return;
    const target = parts.next() orelse return;

    const path = if (std.mem.indexOfScalar(u8, target, '?')) |qp|
        target[0..qp]
    else
        target;

    const body_start = std.mem.indexOf(u8, request, "\r\n\r\n") orelse request.len;
    const body = if (body_start + 4 < request.len) request[body_start + 4 ..] else "";

    const method = std.meta.stringToEnum(std.http.Method, method_str) orelse .GET;

    var wbuf: [65536]u8 = undefined;
    var wpos: usize = 0;

    if (std.mem.startsWith(u8, path, "/api/") or std.mem.eql(u8, path, "/health")) {
        try handleApi(app, fd, &wbuf, &wpos, method, path, body);
    } else {
        try serveStaticFile(fd, &wbuf, &wpos, path);
    }
}

fn writeFd(fd: posix.fd_t, data: [*]const u8, len: usize) !void {
    const rc = std.c.write(fd, data, len);
    if (rc < 0) return error.WriteFailed;
}

fn writeAll(fd: posix.fd_t, buf: *[65536]u8, wpos: *usize, data: []const u8) !void {
    if (wpos.* + data.len > buf.len) {
        try writeFd(fd, buf, wpos.*);
        wpos.* = 0;
    }
    if (data.len > buf.len) {
        try writeFd(fd, data.ptr, data.len);
        return;
    }
    @memcpy(buf[wpos.*..][0..data.len], data);
    wpos.* += data.len;
}

fn flush(fd: posix.fd_t, buf: *[65536]u8, wpos: *usize) !void {
    if (wpos.* > 0) {
        try writeFd(fd, buf, wpos.*);
        wpos.* = 0;
    }
}

fn writeResponse(fd: posix.fd_t, wbuf: *[65536]u8, wpos: *usize, status: []const u8, content_type: []const u8, body: []const u8) !void {
    var hbuf: [1024]u8 = undefined;
    const header = try std.fmt.bufPrint(&hbuf, "HTTP/1.1 {s}\r\nContent-Type: {s}\r\nContent-Length: {d}\r\nConnection: close\r\n\r\n", .{ status, content_type, body.len });
    try writeAll(fd, wbuf, wpos, header);
    try writeAll(fd, wbuf, wpos, body);
    try flush(fd, wbuf, wpos);
}

fn handleApi(app: *const App, fd: posix.fd_t, wbuf: *[65536]u8, wpos: *usize, method: std.http.Method, path: []const u8, body: []const u8) !void {
    _ = app;
    _ = body;

    if (std.mem.eql(u8, path, "/health") and method == .GET) {
        try writeResponse(fd, wbuf, wpos, "200 OK", "application/json", "{\"status\":\"ok\"}");
        return;
    }

    if (std.mem.eql(u8, path, "/api/auth/signup") and method == .POST) {
        try writeResponse(fd, wbuf, wpos, "201 Created", "application/json",
            "{\"message\":\"Check your email to verify your account before signing in.\"}");
        return;
    }

    if (std.mem.eql(u8, path, "/api/auth/signin") and method == .POST) {
        try writeResponse(fd, wbuf, wpos, "200 OK", "application/json",
            "{\"user\":null,\"redirectTo\":\"/\"}");
        return;
    }

    if (std.mem.eql(u8, path, "/api/auth/signout") and method == .POST) {
        try writeResponse(fd, wbuf, wpos, "200 OK", "application/json",
            "{\"message\":\"signed out\"}");
        return;
    }

    if (std.mem.eql(u8, path, "/api/auth/session") and method == .GET) {
        try writeResponse(fd, wbuf, wpos, "200 OK", "application/json",
            "{\"user\":null}");
        return;
    }

    if (std.mem.eql(u8, path, "/api/auth/verify-email") and (method == .GET or method == .POST)) {
        var hbuf: [1024]u8 = undefined;
        const redirect = try std.fmt.bufPrint(&hbuf, "HTTP/1.1 307 Temporary Redirect\r\nLocation: /signin?verified=1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", .{});
        try writeAll(fd, wbuf, wpos, redirect);
        try flush(fd, wbuf, wpos);
        return;
    }

    if (std.mem.eql(u8, path, "/api/auth/forgot-password") and method == .POST) {
        try writeResponse(fd, wbuf, wpos, "200 OK", "application/json",
            "{\"message\":\"if that email exists, a password reset link has been sent\"}");
        return;
    }

    if (std.mem.eql(u8, path, "/api/auth/reset-password") and method == .POST) {
        try writeResponse(fd, wbuf, wpos, "200 OK", "application/json",
            "{\"message\":\"your password has been reset\"}");
        return;
    }

    try writeResponse(fd, wbuf, wpos, "404 Not Found", "application/json", "{\"error\":\"not found\"}");
}

fn serveStaticFile(fd: posix.fd_t, wbuf: *[65536]u8, wpos: *usize, path: []const u8) !void {
    if (std.mem.startsWith(u8, path, "/assets/")) {
        if (std.mem.eql(u8, path, "/assets/index-KaVnzfy_.js")) {
            try serveFile(fd, wbuf, wpos, "200 OK", "application/javascript", js_bundle);
        } else if (std.mem.eql(u8, path, "/assets/index-OWTbmwCm.css")) {
            try serveFile(fd, wbuf, wpos, "200 OK", "text/css", css_bundle);
        } else {
            try writeResponse(fd, wbuf, wpos, "404 Not Found", "text/plain", "Not Found");
        }
    } else {
        try serveFile(fd, wbuf, wpos, "200 OK", "text/html", index_html);
    }
}

fn serveFile(fd: posix.fd_t, wbuf: *[65536]u8, wpos: *usize, status: []const u8, content_type: []const u8, content: []const u8) !void {
    var hbuf: [1024]u8 = undefined;
    const header = try std.fmt.bufPrint(&hbuf, "HTTP/1.1 {s}\r\nContent-Type: {s}\r\nContent-Length: {d}\r\nConnection: close\r\n\r\n", .{ status, content_type, content.len });
    try writeAll(fd, wbuf, wpos, header);
    try writeAll(fd, wbuf, wpos, content);
    try flush(fd, wbuf, wpos);
}
