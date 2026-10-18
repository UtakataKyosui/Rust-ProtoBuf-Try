# 開発環境

## 固定バージョン

| ツール | バージョン | 固定箇所 |
|---|---|---|
| Rust | 1.97.0 (edition 2024) | `rust-toolchain.toml`, CI |
| Node.js | 22.22.0 | `.node-version`, CI |
| pnpm | 10.28.0 | `package.json#packageManager`, CI |
| Vite+ (`vp`) | 1.3.0 | `vite-plus` を `apps/web` / `packages/protocol-ts` で固定 |
| Buf | 1.74.0 | ルート `package.json` |
| `@bufbuild/protobuf` | 2.16.0 | `packages/protocol-ts` |
| Google公式 Rust protobuf / protoc | 未確定 | Phase 1 (G1) で検証後、同一バージョンに固定 |

## コマンド

```sh
pnpm install
pnpm dev          # vp dev
pnpm check        # vp check (format + lint + typecheck)
pnpm test         # vp test run
pnpm build        # vp build
pnpm proto:lint   # buf lint

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## 設計判断

- Reactツールチェーンは VoidZero **Vite+** (`vite-plus` / `vp`) を使い、素のViteとは混同しない。
  `pnpm-workspace.yaml` の `overrides` で `vite` を `@voidzero-dev/vite-plus-core`、`vitest` を同梱版に統一している。
  pnpm の peer 警告 (`unmet peer vite`) は vite-plus 同梱の vite がバージョン表記を持つため出るが、動作には影響しない。
- Cargo workspace は resolver 3 / edition 2024。Google公式 protobuf クレートは beta で protoc の厳密なバージョン一致が必要なため、
  この環境構築ではまだ依存に追加せず、Phase 1 で検証して固定する。
- `proto/` を通信型の単一の定義元とし、CI で `buf lint` を実行する。
