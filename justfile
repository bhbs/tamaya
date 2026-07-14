set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

build:
    cargo build --release

install:
    cargo install --path crates/tamaya-cli --locked

check:
    cargo check

clippy:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    XDG_DATA_HOME=/tmp/tamaya-test-data cargo test --all-features

coverage:
    XDG_DATA_HOME=/tmp/tamaya-test-data cargo llvm-cov --quiet --no-cfg-coverage --all-features --workspace --ignore-filename-regex '(ssh|config).rs$' --fail-under-lines 99

ci: fmt-check clippy test

clippy-fix:
    cargo clippy --all-targets --all-features --fix --allow-dirty --allow-staged

ci-fix: fmt clippy-fix

bump semver:
    #!/usr/bin/env bash
    set -euo pipefail
    CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
    IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"
    case "{{semver}}" in
      major) MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0 ;;
      minor) MINOR=$((MINOR + 1)); PATCH=0 ;;
      patch) PATCH=$((PATCH + 1)) ;;
      *) echo "Usage: just bump [patch|minor|major]"; exit 1 ;;
    esac
    NEW_VERSION="$MAJOR.$MINOR.$PATCH"
    sed -i '' "s/^version = \"$CURRENT\"/version = \"$NEW_VERSION\"/" Cargo.toml
    cargo check
    git add Cargo.toml Cargo.lock
    git commit -m "v${NEW_VERSION}"
    git tag -a "v${NEW_VERSION}" -m "v${NEW_VERSION}"
    git push origin main "v${NEW_VERSION}"
    echo "✅ Bumped to v${NEW_VERSION}"

# linux-musl cross-build (macOS / Linux): https://github.com/rust-cross/cargo-zigbuild
linux-musl-target := "x86_64-unknown-linux-musl"

ensure-linux-musl-target:
    rustup target add {{linux-musl-target}}

check-zigbuild:
    @command -v zig >/dev/null || { echo "zig not found; install from https://ziglang.org/ or: brew install zig"; exit 1; }
    @command -v cargo-zigbuild >/dev/null || { echo "cargo-zigbuild not found; run: cargo install cargo-zigbuild --locked"; exit 1; }

run *args:
    cargo run -- {{args}}

build-demo-spa outdir="dist":
    cd demo/spa && npm install && npm run build
    @echo "✅ Demo build complete → demo/spa/{{outdir}}/demo"

build-demo-go outdir="dist":
    rm -rf demo/go/{{outdir}}
    cd demo/spa && npm install && npm run build
    rm -rf demo/go/static
    cp -R demo/spa/dist/demo demo/go/static
    mkdir -p demo/go/{{outdir}}
    cd demo/go && GOOS=linux GOARCH=amd64 go build -o {{outdir}}/demo .
    @echo "✅ Demo build complete → demo/go/{{outdir}}/demo"

build-demo-rust outdir="dist": check-zigbuild ensure-linux-musl-target
    rm -rf demo/rust/{{outdir}}
    cd demo/spa && npm install && npm run build
    rm -rf demo/rust/static
    cp -R demo/spa/dist/demo demo/rust/static
    mkdir -p demo/rust/{{outdir}}
    cd demo/rust && cargo zigbuild --target {{linux-musl-target}} --release
    @cp demo/rust/target/x86_64-unknown-linux-musl/release/demo demo/rust/{{outdir}}/demo
    @echo "✅ Demo build complete → demo/rust/{{outdir}}/demo"

build-demo-bun outdir="dist":
    rm -rf demo/bun/{{outdir}}
    mkdir -p demo/bun/{{outdir}}
    cd demo/bun && bun install && bun run build:linux
    @echo "✅ Demo build complete → demo/bun/{{outdir}}"

node-version := "v26.0.0"
node-target-toolchain := "demo/node/vendor/node-" + node-version + "-linux-x64"

download-demo-node-toolchain:
    mkdir -p demo/node/vendor
    @host_os="$(uname -s | tr '[:upper:]' '[:lower:]')"; host_arch="$(uname -m | sed 's/^arm64$/arm64/; s/^aarch64$/arm64/; s/^x86_64$/x64/')"; host_toolchain="demo/node/vendor/node-{{node-version}}-$host_os-$host_arch"; \
      [ -x {{node-target-toolchain}}/bin/node ] || { curl -fsSL https://nodejs.org/dist/{{node-version}}/node-{{node-version}}-linux-x64.tar.xz | tar -xJ -C demo/node/vendor; }; \
      [ -x "$host_toolchain/bin/node" ] || { curl -fsSL "https://nodejs.org/dist/{{node-version}}/node-{{node-version}}-$host_os-$host_arch.tar.xz" | tar -xJ -C demo/node/vendor; }

build-demo-node outdir="dist": download-demo-node-toolchain
    rm -rf demo/node/{{outdir}}
    mkdir -p demo/node/{{outdir}}
    cd demo/node && host_os="$(uname -s | tr '[:upper:]' '[:lower:]')" && host_arch="$(uname -m | sed 's/^arm64$/arm64/; s/^aarch64$/arm64/; s/^x86_64$/x64/')" && PATH="$PWD/vendor/node-{{node-version}}-$host_os-$host_arch/bin:$PATH" npm install && PATH="$PWD/vendor/node-{{node-version}}-$host_os-$host_arch/bin:$PATH" npm install --no-save --cpu=wasm32 sharp@0.34.5 && PATH="$PWD/vendor/node-{{node-version}}-$host_os-$host_arch/bin:$PATH" NEXT_SEA_NODE=vendor/node-{{node-version}}-linux-x64/bin/node NEXT_SEA_OUTPUT={{outdir}}/demo npm run build
    @echo "✅ Demo build complete with WASM Sharp assets → demo/node/{{outdir}}/demo"

build-demo-zig outdir="dist": check-zigbuild
    rm -rf demo/zig/{{outdir}}
    cd demo/spa && npm install && npm run build
    rm -rf demo/zig/src/static
    cp -R demo/spa/dist/demo demo/zig/src/static
    mkdir -p demo/zig/{{outdir}}
    @cd demo/zig && [ -f sqlite3.c ] || { curl -fsSLo z.zip https://www.sqlite.org/2024/sqlite-amalgamation-3460100.zip && unzip -j -qo z.zip "*/sqlite3.c" "*/sqlite3.h" && rm -f z.zip; }
    cd demo/zig && zig build -Dtarget=x86_64-linux-musl -Doptimize=ReleaseSafe
    @cp demo/zig/zig-out/bin/demo demo/zig/{{outdir}}/demo
    @echo "✅ Demo build complete → demo/zig/{{outdir}}/demo"

dev-demo-go port="8080":
    cd demo/spa && npm install && npm run build
    rm -rf demo/go/static
    cp -R demo/spa/dist/demo demo/go/static
    cd demo/go && PORT={{port}} go run .

dev-demo-node port="3000":
    cd demo/node && npm install && PORT={{port}} npm run dev

dev-demo-rust port="8080":
    cd demo/spa && npm install && npm run build
    rm -rf demo/rust/static
    cp -R demo/spa/dist/demo demo/rust/static
    cd demo/rust && PORT={{port}} cargo run

dev-demo-bun port="3000":
    cd demo/bun && bun install && PORT={{port}} bun run dev

dev-demo-zig port="8080":
    cd demo/spa && npm install && npm run build
    rm -rf demo/zig/src/static
    cp -R demo/spa/dist/demo demo/zig/src/static
    @cd demo/zig && [ -f sqlite3.c ] || { curl -fsSLo z.zip https://www.sqlite.org/2024/sqlite-amalgamation-3460100.zip && unzip -j -qo z.zip "*/sqlite3.c" "*/sqlite3.h" && rm -f z.zip; }
    cd demo/zig && PORT={{port}} zig build run -Doptimize=Debug

deploy-demo-spa: build-demo-spa
    cargo run -- --project-dir demo/spa publish
    @echo "✅ Demo SPA deployed"

deploy-demo-go: build-demo-go
    cargo run -- --project-dir demo/go deploy
    @echo "✅ Demo Go deployed"

deploy-demo-rust: build-demo-rust
    cargo run -- --project-dir demo/rust deploy
    @echo "✅ Demo Rust deployed"

deploy-demo-bun: build-demo-bun
    cargo run -- --project-dir demo/bun deploy
    @echo "✅ Demo Bun deployed"

deploy-demo-node: build-demo-node
    echo '/var/lib/tamaya/apps/demo-node/data/next-sea-cache' | cargo run -- --project-dir demo/node env set NEXT_SEA_CACHE_DIR --stdin
    cargo run -- --project-dir demo/node deploy
    @echo "✅ Demo Node deployed"

deploy-demo-zig: build-demo-zig
    cargo run -- --project-dir demo/zig deploy
    @echo "✅ Demo Zig deployed"

deploy-all-demos: deploy-demo-spa deploy-demo-go deploy-demo-rust deploy-demo-bun deploy-demo-node deploy-demo-zig
    @echo "✅ All demos deployed"
