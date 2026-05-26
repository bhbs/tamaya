# Firecracker ベース軽量 PaaS / VM Platform 設計メモ

## ゴール

1 VPS 上で大量の小規模アプリを動かすための lightweight platform。

優先事項:

- deploy の簡単さ
- isolation
- rollback
- SQLite 永続化
- 運用コスト削減
- immutable infrastructure
- プロトタイプ大量配置

非優先:

- multi-region
- distributed consensus
- auto scaling
- multi-writer DB
- 複数ホスト orchestration
- Kubernetes compatibility

---

## 基本思想

### control plane と worker を分ける

`v` CLI は Mac 上で実行する controller。

Firecracker の実行、API socket 操作、TAP/bridge 作成、iptables/nftables、image/volume/runtime の実体管理は SSH 先の Linux worker で行う。

```text
Mac
 └── v CLI
      └── ssh
           └── Linux worker / VPS
                ├── firecracker process
                ├── Firecracker API socket
                ├── kernel/rootfs images
                ├── persistent volumes
                ├── TAP/bridge networking
                └── reverse proxy
```

Mac 側の controller 状態は XDG Base Directory Specification に従って置く。Firecracker が直接読む path ではない。

```text
$XDG_CONFIG_HOME/v/config.toml        # fallback: ~/.config/v/config.toml
$XDG_DATA_HOME/v/images/              # fallback: ~/.local/share/v/images/
$XDG_DATA_HOME/v/volumes/             # fallback: ~/.local/share/v/volumes/
$XDG_STATE_HOME/v/registry.toml       # fallback: ~/.local/state/v/registry.toml
$XDG_RUNTIME_DIR/v/                   # fallback: ~/.local/state/v/runtime/v/
```

Linux worker 側の path は Firecracker から見える host path。`--kernel`、`--rootfs`、volume path、runtime socket path は worker 上の path として扱う。

worker は controller 側の `config.toml` に定義する。

```toml
default_worker = "vps-prod"

[workers.vps-prod]
host = "203.0.113.10"
user = "deploy"
port = 22
identity_file = "/Users/me/.ssh/id_ed25519"
firecracker_bin = "/usr/local/bin/firecracker"
```

`user`、`port`、`identity_file` は任意。

worker 側も XDG に従う。`v run` は SSH 先で `$XDG_DATA_HOME`、`$XDG_STATE_HOME`、`$XDG_RUNTIME_DIR` を解決し、未設定なら spec の fallback を使う。

---

### immutable rootfs

アプリ本体は immutable image。

例:

```text
${XDG_DATA_HOME:-$HOME/.local/share}/v/images/myapp-v42.ext4
```

更新時は image を差し替える。

---

### persistent data volume

永続データは host 側 volume。

例:

```text
${XDG_DATA_HOME:-$HOME/.local/share}/v/volumes/myapp/
```

内容:

```text
db.sqlite
uploads/
cache/
```

---

## Firecracker VM 構成

```text
Linux worker / VPS
 ├── vm image
 │    └── app-v42.ext4
 │
 ├── persistent volume
 │    └── ${XDG_DATA_HOME:-$HOME/.local/share}/v/volumes/myapp/
 │
 └── Firecracker microVM
      ├── rootfs (read-only)
      └── /data (persistent)
```

---

## ストレージ戦略

### DB

SQLite 使用。

推奨:

```sql
PRAGMA journal_mode=WAL;
```

---

### uploads

uploads は host volume。

---

### rootfs にデータを書かない

rootfs は disposable。

永続化禁止。

---

## 超重要制約

### single writer

SQLite volume は:

```text
1 volume = 1 active writer VM
```

を原則とする。

---

## ゼロダウンタイム deploy

### deploy sequence

```text
1. new VM 起動
2. health check
3. traffic switch
4. old VM drain
5. old VM stop
```

---

## volume attach policy

同じ ext4 volume を:

```text
複数 VM 同時 RW mount
```

しない。

ext4 は cluster filesystem ではない。

---

## 推奨 mount 構成

### DB

dedicated block device 推奨。

---

### uploads

別 block device、または guest agent + vsock 経由で扱う。

例:

```text
db.sqlite -> block device
uploads/ -> block device
```

---

## 避けるもの

### overlayfs upperdir に SQLite

避ける。

理由:

- copy-up
- fsync semantics
- corruption risk

---

## 推奨 architecture

### Mac controller responsibilities

Mac の `v` CLI が担当:

- worker 接続設定
- SSH 経由の worker 操作
- deploy / rollback の orchestration
- worker に置く image / volume / runtime path のメタデータ管理
- app registry
- deploy lock

---

### Linux worker responsibilities

Linux worker が担当:

- image storage
- volume management
- VM lifecycle
- reverse proxy
- health check
- Firecracker API socket 操作
- TAP/bridge/iptables/nftables 操作

---

### VM responsibilities

VM が担当:

- app process
- local SQLite access
- HTTP server

---

## reverse proxy

候補:

- Caddy
- nginx
- Traefik

役割:

- TLS termination
- routing
- deploy switching
- health check

---

## deploy model

### immutable deploy

毎回:

```text
new image
↓
new VM
↓
switch
↓
old VM destroy
```

---

## rollback

rollback は:

```text
old image boot
```

のみ。

---

## snapshot

将来的に:

- Firecracker snapshot
- fast cold start

を検討可能。

---

## volume layout example

```text
${XDG_DATA_HOME:-$HOME/.local/share}/v/volumes/
 ├── app-a/
 │    ├── db.sqlite
 │    └── uploads/
 │
 ├── app-b/
 │    ├── db.sqlite
 │    └── uploads/
```

---

## image layout example

```text
${XDG_DATA_HOME:-$HOME/.local/share}/v/images/
 ├── app-a-v1.ext4
 ├── app-a-v2.ext4
 ├── app-b-v7.ext4
```

---

## orchestration philosophy

Kubernetes 的な巨大 orchestration は目指さない。

目的:

```text
single VPS
+
many isolated apps
+
simple deploy
```

---

## 将来的な拡張

必要になったら:

- remote volume
- PostgreSQL
- multi-host scheduler
- distributed routing
- live migration
- backup / restore
- DB schema migration strategy

---

## 検証方法

### 1. ビルドを確認する

```bash
cargo build
cargo build --release
```

### 2. テストを実行する

```bash
cargo test
```

### 3. CLI ヘルプを確認する

```bash
cargo run -- --help
cargo run -- init --help
cargo run -- run --help
cargo run -- deploy example-app --help
cargo run -- rollback example-app --help
cargo run -- stop example-app --help
cargo run -- logs example-app --help
cargo run -- ps --help
```

### 4. CLI 引数パースを安全に確認する

```bash
cargo run -- run web --kernel /kernels/vmlinux --rootfs /images/web.ext4 --dry-run
```

`--dry-run` オプションは、Firecracker 起動を行わずに CLI の引数パースと boot plan 生成を確認するための暫定機能。

ここで指定する `/kernels/vmlinux` や `/images/web.ext4` は、本来は SSH 先 Linux worker 上の path として解釈する。Mac ローカルの path ではない。

### 5. remote worker 実行の想定

最終的な実行モデルは、Mac から SSH 越しに Linux worker の Firecracker を操作する形。

例:

```bash
cargo run -- run web \
  --worker vps-prod \
  --kernel /kernels/vmlinux \
  --rootfs '${XDG_DATA_HOME:-$HOME/.local/share}/v/images/web.ext4' \
  --tap tap0 \
  --vcpu 1 \
  --memory-mib 256
```

このコマンドは次のことを行います:

- Mac 上の `v` CLI が worker 設定を読む
- SSH で Linux worker に接続する
- worker 上の runtime directory に Firecracker API socket を生成する
- worker 上で `firecracker` プロセスを起動する
- ルートファイルシステムを read-only でアタッチ
- worker 上の TAP ネットワークインターフェースを構成
- microVM をブート

worker 側の前提:

- Linux/KVM が利用できる
- Firecracker バイナリが worker にインストール済み
- worker 上に kernel/rootfs image が存在する
- TAP/bridge/network namespace を作成できる権限がある
- volume path は worker 上の path
- reverse proxy reload も worker 上で実行する

非 `--dry-run` 実行時は、boot に進む前に SSH 越しに worker directory 作成と capability check を行う。

確認する内容:

- `uname -s` が `Linux`
- `/dev/kvm` が存在し、読み書き可能
- `sh` と `ip` コマンドが存在
- worker config の `firecracker_bin` が実行可能、または `PATH` から解決可能
worker 上で以下を作成する。

```text
${XDG_DATA_HOME:-$HOME/.local/share}/v/images/
${XDG_DATA_HOME:-$HOME/.local/share}/v/volumes/
${XDG_STATE_HOME:-$HOME/.local/state}/v/
${XDG_RUNTIME_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/v/runtime}/v/<app>/
${XDG_RUNTIME_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/v/runtime}/v/<app>/logs/
```

Mac 側の前提:

- SSH 接続できる
- worker 接続設定が XDG config path の `v/config.toml` にある
- Mac に Firecracker や KVM は不要

### 6. 現在の実装との差分

現在の実装はまだ暫定で、`v run` がローカルで Firecracker バイナリを起動し、ローカル Unix domain socket に直接 API を送る。

これは最終仕様ではない。次に直すべき点:

- `FirecrackerProcess::start` 相当を SSH 上のプロセス起動に置き換える
- Firecracker API socket への HTTP request を SSH 経由で worker 上から送る
- runtime state の `pid` と `api_socket` を worker 上の値として保存する
- `stop` / `logs` / `ps` も worker 越しに実行する

`--worker` と worker config、SSH runner、worker capability check、worker 側 runtime directory 作成、worker 側 runtime/API socket path の組み立ては実装済み。現状で `--firecracker-bin` は過去のローカル起動実装由来の暫定オプション。最終仕様では worker config の `firecracker_bin` を使う。

非 `--dry-run` で実行すると、remote worker 実行が未実装であることを示すエラーになる:

```text
Error: remote worker execution is not implemented yet; run with --dry-run to inspect the boot plan
```

### 7. 停止・状態確認する

```bash
cargo run -- ps
cargo run -- stop web
```

最終仕様では、`ps` は registry と worker runtime state を照合し、`stop` は SSH 越しに worker 上の Firecracker プロセスを終了して runtime state を削除する。

### 8. まだ実装されていないコマンド

現在の実装では、以下のコマンドは動作を確認できますが、実際の処理はまだスタブです:

- `cargo run -- deploy myapp`
- `cargo run -- rollback myapp`
- `cargo run -- logs myapp`

これらは現時点で「未実装」と表示されます。

### 9. サブコマンド構成を確認する

```bash
cargo run -- --help
cargo run -- init --help
cargo run -- run --help
cargo run -- deploy example-app --help
cargo run -- rollback example-app --help
cargo run -- stop example-app --help
cargo run -- logs example-app --help
cargo run -- ps --help
```

これらのコマンドで `clap` によるサブコマンド引数パースが正常に動くことを確認できます。

---

## MVP 優先順位

### Phase 1

- Firecracker boot
- immutable image
- persistent volume
- reverse proxy
- deploy / rollback
- health check

---

### Phase 2

- snapshot boot
- log aggregation
- metrics
- resource limit UI

---

### Phase 3

- multi-host
- migration
- remote storage
- HA

---

## 技術候補

### VM

- Firecracker

---

### guest init

- systemd
- custom init

---

### networking

- TAP device
- bridge
- iptables/nftables

---

### storage

- ext4
- virtio-blk
- virtio-vsock

---

### image build

候補:

- docker export
- debootstrap
- alpine rootfs
- nix
- custom builder

---

## この構成のメリット

- 軽量
- rollback が簡単
- app isolation
- rootfs immutable
- SQLite 使用可能
- VPS コスト最小化
- 大量 deploy 向き

---

## この構成の限界

- multi-writer 不向き
- distributed system ではない
- host 障害で全停止
- volume failover なし
- Kubernetes 的 elasticity なし

---

## まとめ

目指すもの:

```text
Fly.io と Heroku の中間くらいを
single VPS 上で超軽量にやる
```

思想:

```text
small
simple
immutable
replaceable
single-writer
SQLite-friendly
```
