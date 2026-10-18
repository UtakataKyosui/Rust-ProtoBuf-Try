# Protobuf 基盤(Epic #2)

`proto/` を通信型の**単一の定義元**とし、Rust(Google公式)と TypeScript(Protobuf-ES)の型を生成する。
ts-rs / specta は採用しない。

## 1. Google公式 Rust Protobuf の検証結果 (#9)

| 項目 | 結論 |
|---|---|
| ランタイム | `protobuf = "=4.36.2-release"`(内部で `google-protobuf 0.36.2-release` を再公開。公開依存はこの1本に統一) |
| コード生成 | `protobuf-codegen = "=4.36.2-release"`(`build.rs` から `CodeGen` を呼ぶ) |
| protoc | **36.2**(クレート `x.y.z` ↔ protoc `y.z` の厳密一致。codegen が実行時に検証し、不一致なら panic) |
| カーネル | upb(C)。`google-protobuf` の `build.rs` が `cc` で `libupb` をビルドするため **C コンパイラ(gcc/clang)が必要**。CMake / Bazel は不要 |
| `0.37.0-rc.1` 系 | `google-protobuf*` 名義の rc のみで、対応する protoc 37.0-rc1 のビルド済みバイナリが GitHub Releases に無いため不採用 |
| API | `Message::new()` / `set_xxx()` / `xxx()`(View)、`Serialize::serialize()` / `Parse::parse()`、oneof は `has_xxx()` と `payload()` |
| 生成物 | `$OUT_DIR/protobuf_generated/protolive/v1/generated.rs` を `include!`(`apps/server/src/lib.rs`) |
| Clippy | 生成コードは `needless_borrow` 等を出すため、`proto` モジュールのみ `#[allow(clippy::all, warnings)]` |

動作確認: Linux x86_64 でビルド・テスト成功。macOS は同じ手順(Xcode CLT の clang + 同バージョンの protoc)で動く想定だが、このリポジトリのCIは Linux のみ。

### protoc の入手

```sh
# Linux x86_64 の例。macOS は protoc-36.2-osx-universal_binary.zip
curl -sSLO https://github.com/protocolbuffers/protobuf/releases/download/v36.2/protoc-36.2-linux-x86_64.zip
unzip protoc-36.2-linux-x86_64.zip -d ~/.local   # bin/protoc を PATH に通す
protoc --version   # libprotoc 36.2
```

`PROTOC` 環境変数で任意のパスも指定できる。

## 2. スキーマ (#10)

`proto/protolive/v1/chat.proto`(package `protolive.v1`、proto3)。

- `ClientEnvelope` { `join: JoinRequest` | `send_chat: SendChatRequest` }
- `ServerEnvelope` { `joined` / `left: MemberEvent` | `chat: ChatMessage` | `error: ErrorEvent` }
- 1 WebSocket Binary フレーム = 1 Envelope(長さプレフィックスなし)。
- メッセージ ID・送信者 ID・`timestamp_ms` は**サーバーが採番**する。クライアント入力(nickname / content)は信頼しない。
- 将来用に Envelope と `ChatMessage` に `reserved` 範囲を確保。削除したフィールド番号は再利用しない。

```sh
pnpm proto:lint       # buf lint (STANDARD)
pnpm proto:breaking   # buf breaking (FILE) vs origin/main
```

CI の `proto` ジョブで lint を、PR では breaking も実行する。

## 3. コード生成 (#11)

| 言語 | 方法 | 生成物の扱い |
|---|---|---|
| Rust | `apps/server/build.rs` → `protobuf-codegen`(`cargo build` で自動) | `OUT_DIR` のみ。コミットしない |
| TypeScript | `buf generate`(`buf.gen.yaml`、`@bufbuild/protoc-gen-es` 2.16.0、`target=ts`) | `packages/protocol-ts/src/gen`。`.gitignore` 済みでコミットしない |

```sh
pnpm proto:generate   # TS 生成のみ
pnpm check / test / build   # protocol-ts は実行前に自動で再生成する
```

生成物をコミットしないため、クリーン環境でも `pnpm install && pnpm build` / `cargo build` で再現できる。

## 4. 相互運用テスト (#12)

`fixtures/golden/*.bin` が正本のバイナリ。

- Rust: `apps/server/tests/golden.rs` — 構築 → エンコード結果が fixture と一致、fixture → デコード内容が一致。
- TypeScript: `packages/protocol-ts/src/golden.test.ts` — 同じ内容を構築して同様に検証。
- 観点: 各 oneof、日本語・絵文字、空メッセージ、長さ境界(127 / 128 / 16384 バイト)、未知フィールド(無視しつつ保持して再エンコード)、不正 bytes(切り詰め・不正ワイヤタイプ・不正 UTF-8)の拒否。

fixture の再生成(スキーマ変更時のみ):

```sh
UPDATE_GOLDEN=1 cargo test -p protolive-server --test golden
```

その後 `pnpm test` と `cargo test` の双方で差分がないことを確認する。
