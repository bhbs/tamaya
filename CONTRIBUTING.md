# Contributing to Tamaya

Thank you for your interest in contributing to Tamaya!

## Development Setup

Requires Rust 1.95 or newer and [just](https://github.com/casey/just).

```bash
git clone https://github.com/bhbs/tamaya.git
cd tamaya
just ci
```

## Common Commands

```bash
just fmt          # Format code
just clippy       # Run linter
just test         # Run tests
just ci           # Run fmt-check, clippy, and tests (CI pipeline)
just coverage     # Run tests with coverage (requires cargo-llvm-cov)
```

## Making Changes

1. Fork the repository and create a feature branch.
2. Make your changes.
3. Run `just ci` and ensure it passes.
4. Submit a pull request.

## Code Style

- Follow standard Rust formatting (`rustfmt`).
- No warnings allowed (`clippy -D warnings`).
- Tests are required for new functionality.
- Coverage target is 99% line coverage.

## Reporting Issues

Use [GitHub Issues](https://github.com/bhbs/tamaya/issues) to report bugs or request features.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
