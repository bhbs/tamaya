const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const mod = b.createModule(.{
        .root_source_file = b.path("src/main.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = true,
    });

    const exe = b.addExecutable(.{
        .name = "demo",
        .root_module = mod,
    });

    exe.root_module.addIncludePath(b.path("."));
    exe.root_module.addCSourceFile(.{ .file = b.path("sqlite3.c"), .flags = &.{"-std=c99"} });

    b.installArtifact(exe);
}
