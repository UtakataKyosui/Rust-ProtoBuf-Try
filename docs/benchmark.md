# JSON / Protobuf ベンチマーク (#19)

「Protobuf は常に JSON より小さい・速い」とは限らないため、条件つきで結果を記録する。
**シリアライズ単体**の測定であり、WebSocket を含む E2E 遅延や負荷(接続数・送信頻度)は含まない。

## 再現手順

```sh
# 要: protoc 36.2 (docs/protobuf.md)
cargo bench -p protolive-server --bench codec
```

- 実装: `apps/server/benches/codec.rs`(Criterion 0.7、既定設定 = warm-up 3 秒 / 100 サンプル / 測定約 5 秒)。
- 起動時にペイロードのバイト数表を標準エラーへ出力し、続けて Criterion が時間を出力する。HTML レポートは `target/criterion/`。
- Protobuf: Google 公式 Rust `protobuf =4.36.2-release`(upb カーネル)の生成型。
  JSON: `serde` + `serde_json 1`(コンパクト出力、非 ASCII は UTF-8 のまま)。JSON は通信路では使わず、`dev-dependencies` のみ。
- 対象は同じ論理メッセージ。サーバー → クライアントの `ServerEnvelope.chat`(id・sender_id は UUID、nickname「ゆうき」、
  timestamp 1760000000123)と、クライアント → サーバーの `ClientEnvelope.send_chat`。本文の長さと文字種を 4 通り変える。
- 測定項目: `encode`(構築済みの値をバイト列へ)、`decode`(バイト列を値へ。検証前の構造デコードのみ)、
  `build_encode`(値の構築+encode。実サーバーが毎回行う処理に近い)。

## 測定環境(この結果の取得条件)

| 項目 | 値 |
|---|---|
| CPU | Intel Xeon @ 2.10GHz (4 vCPU、クラウド VM。共有環境のためばらつきあり) |
| メモリ | 16 GiB |
| OS | Linux 6.18 |
| Rust | 1.97.0 (`--release` 相当の `bench` プロファイル) |
| 日付 | 2026-10-18 |

絶対値は環境依存。**比較は同一マシン・同一実行内の相対値**として読むこと。

## ペイロードサイズ(バイト)

| ケース(本文) | サーバー→クライアント JSON | Protobuf | 比 | クライアント→サーバー JSON | Protobuf | 比 |
|---|---:|---:|---:|---:|---:|---:|
| short_ascii (`hello`) | 175 | 103 | 58.9% | 33 | 9 | 27.3% |
| short_japanese (9 文字) | 195 | 123 | 63.1% | 53 | 29 | 54.7% |
| medium_ascii_200 (200 文字) | 370 | 300 | 81.1% | 228 | 206 | 90.4% |
| long_japanese_1000 (1000 文字) | 3170 | 3100 | 97.8% | 3028 | 3006 | 99.3% |

## 処理時間(中央値、ns / µs)

| ケース | 操作 | Protobuf | JSON | Protobuf / JSON |
|---|---|---:|---:|---:|
| short_ascii | encode | 158 ns | 260 ns | 0.61 |
| | decode | 242 ns | 329 ns | 0.74 |
| | build+encode | 765 ns | 293 ns | **2.61** |
| short_japanese | encode | 161 ns | 295 ns | 0.55 |
| | decode | 271 ns | 354 ns | 0.77 |
| | build+encode | 620 ns | 392 ns | **1.58** |
| medium_ascii_200 | encode | 276 ns | 407 ns | 0.68 |
| | decode | 322 ns | 420 ns | 0.77 |
| | build+encode | 676 ns | 572 ns | **1.18** |
| long_japanese_1000 | encode | 350 ns | 2.77 µs | 0.13 |
| | decode | 1.76 µs | 3.08 µs | 0.57 |
| | build+encode | 1.22 µs | 2.95 µs | 0.41 |

Criterion の信頼区間は各値の ±5〜15% 程度。差が小さい行(例: medium の build+encode)は誤差の範囲として読む。

## 読み取れること・限界

- **サイズ**: 本文が短いほど Protobuf の利得が大きい(フィールド名が JSON に含まれないため)。
  本文が支配的になる長文では差はほぼ消える。UUID を文字列で持つため id 系フィールドが固定で約 80 バイトを占める。
  `bytes` 型 16 バイトにすればさらに縮められるが、ブラウザ側の扱いやすさとのトレードオフで MVP は文字列。
- **encode / decode 単体**: Protobuf が一貫して速い(0.55〜0.77 倍、長文の encode は 0.13 倍)。
- **値の構築を含めると逆転する**: 短文では Protobuf が JSON の 1.2〜2.6 倍遅い。公式 Rust API のセッター呼び出し
  (upb の arena 確保・文字列コピー)のコストが支配的で、serde は参照のまま書き出せるため。
  サーバーは配信ごとに 1 回だけ構築・encode して全受信者に同じバイト列を送る設計なので、この差は受信者数で償却される。
- 計測していないもの: WebSocket E2E 遅延、メモリ使用量、接続数・送信頻度を変えた負荷、ブラウザ側(Protobuf-ES / `JSON.parse`)の性能。
  これらは MVP の範囲外で、必要になった時点で別途測定する。
