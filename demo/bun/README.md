# demo/bun

This demo compiles a Next.js application into a Linux executable with [Bun
compile](https://bun.com/docs/bundler/executables) and
[next-bun-compile](https://github.com/ramonmalcolm10/next-bun-compile).

Bun executable support is planned after Tamaya v1. The v1 runtime contract is a
single Linux executable that listens on `PORT` and exposes `GET /health`.
