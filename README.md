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

### immutable rootfs

アプリ本体は immutable image。

例:

```text
/vm-images/myapp-v42.ext4
```

更新時は image を差し替える。

---

### persistent data volume

永続データは host 側 volume。

例:

```text
/volumes/myapp/
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
Host
 ├── vm image
 │    └── app-v42.ext4
 │
 ├── persistent volume
 │    └── /volumes/myapp/
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

### Host responsibilities

Host が担当:

- image storage
- volume management
- VM lifecycle
- reverse proxy
- health check

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
/volumes/
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
/vm-images/
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
- health check failure rollback
- volume lock management

を追加。

最初からやらない。

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
