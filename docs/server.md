# サーバー設計(Rust WebSocket チャットサーバー)

`apps/server`。Axum + Tokio。通信は WebSocket Binary + proto3(`proto/protolive/v1/chat.proto`)。

## 起動

```sh
PROTOLIVE_ADDR=127.0.0.1:3000 RUST_LOG=info cargo run -p protolive-server   # ws://127.0.0.1:3000/ws
```

`PROTOLIVE_ADDR` 省略時は `127.0.0.1:3000`。Ctrl-C で全セッションへ Close(1001) を送り graceful shutdown する。

## 構成

| モジュール | 役割 |
|---|---|
| `server` | `/ws` の upgrade、セッションループ(受信・配信・ping・shutdown を `select!`)、接続ごとの UUID |
| `room` | インメモリ単一ルーム。有界 `broadcast`(既定 256 件)で Join/Leave/Chat を全参加者へ配信 |
| `adapter` | 生成型とドメイン型の分離、入力検証、`ErrorEvent` への変換。生成型はこのモジュール外へ出さない |

## 振る舞い

- Binary フレームのみ受理。Text は `ErrorEvent` 後に Close(1003)。
- 1 メッセージ上限 16 KiB(`ServerConfig::max_message_bytes`)。超過した接続のみ切断され、他接続に影響しない。
- 構造が壊れた Protobuf は `malformed_message` を返して Close(1007)。値の検証エラー
  (`invalid_nickname` / `invalid_content` / `not_joined` / `already_joined`)は `ErrorEvent` を返して接続を維持する。
- 検証: ニックネーム 1〜32 文字(前後空白除去、制御文字不可)、本文 1〜1000 文字(空白のみ不可、改行・タブ以外の制御文字不可)。
  文字数は Unicode scalar value 数。
- 送信者 ID・メッセージ ID・時刻はサーバーが付与する(クライアント値は使わない)。
- 参加前は配信を購読せず、発言も拒否する。参加者本人にも自分の `Joined` が届く。
- 遅い受信者が broadcast バッファから溢れると `lagged` を送って Close(1013) で切断する(再接続前提)。
- 切断・異常終了のどの経路でも退出処理を通り、他参加者へ `Left` が配信される。
- サーバーは 30 秒ごとに Ping を送り、90 秒無通信で Close(1008)。Pong は axum が自動応答する。
- ログは `tracing`(`RUST_LOG` で制御)。

## テスト

- `adapter` / `room` の単体テスト。
- `tests/ws.rs`: 実サーバーを起動して 2 クライアントの配信、不正入力、Text 拒否、サイズ超過、lag 切断、graceful shutdown を検証。
