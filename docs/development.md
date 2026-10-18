# 開発環境

## 固定バージョン

| ツール | バージョン | 固定箇所 |
|---|---|---|
| Rust | 1.97.0 (edition 2024) | `rust-toolchain.toml`, CI |
| Node.js | 22.22.0 (`engines`: `^22.18.0 || >=24.11.0`、Vite+の要件) | `.node-version`, `package.json`, CI |
| pnpm | 10.28.0 | `package.json#packageManager`, CI |
| Vite+ (`vp`) | 1.3.0 | `vite-plus` を `apps/web` / `packages/protocol-ts` で固定 |
| Buf | 1.74.0 | ルート `package.json` |
| `@bufbuild/protobuf` | 2.16.0 | `packages/protocol-ts` |
| Google公式 Rust protobuf | 4.36.2-release | `apps/server/Cargo.toml`(`=` 固定。詳細は [protobuf.md](protobuf.md)) |
| protoc | 36.2 | CI(`PROTOC_VERSION`)、手元は PATH / `PROTOC` |

## コマンド

```sh
pnpm install
pnpm dev          # vp dev
pnpm check        # vp check (format + lint + typecheck; vite.config.tsの lint.options.typeAware/typeCheck で型検査を有効化)
pnpm test         # vp test run
pnpm build        # vp build
pnpm proto:lint   # buf lint
pnpm proto:generate  # buf generate (TS)

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## 設計判断

- Reactツールチェーンは VoidZero **Vite+** (`vite-plus` / `vp`) を使い、素のViteとは混同しない。
  `pnpm-workspace.yaml` の `overrides` で `vite` を `@voidzero-dev/vite-plus-core`、`vitest` を同梱版に統一している。
  pnpm の peer 警告 (`unmet peer vite`) は vite-plus 同梱の vite がバージョン表記を持つため出るが、動作には影響しない。
- Cargo workspace は resolver 3 / edition 2024。Google公式 protobuf クレートは beta で protoc の厳密なバージョン一致が必要なため、
  `protobuf` / `protobuf-codegen` を `=4.36.2-release` に固定し、protoc 36.2 を必要とする(`docs/protobuf.md`)。
- `proto/` を通信型の単一の定義元とし、CI で `buf lint` を実行する。

## フォローアップ(Phase 1 で対応)

- Issue #8 の「Protoc / Protobuf のバージョン固定」は #9 の検証結果(protobuf 4.36.2-release / protoc 36.2)で確定した。
- Rust 側の `banner` テストはスモークレベル。サーバー実装(Phase 2)で実質的なテストに置き換える。
