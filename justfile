set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    just --list

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

check:
    cargo check

clippy:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo test --all-features

coverage:
    cargo llvm-cov --quiet --no-cfg-coverage --all-features --workspace --fail-under-lines 99

ci: fmt-check clippy test coverage

run *args:
    cargo run -- {{args}}
