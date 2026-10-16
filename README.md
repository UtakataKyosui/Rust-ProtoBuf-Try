ProtoLive

Real-time chat powered by Rust, WebSocket, and Google’s official Protocol Buffers implementation.

ProtoLiveは、Rustでリアルタイムチャットアプリケーションを構築し、Google公式のRust向けProtocol Buffers実装を実践的に評価する実験プロジェクトです。

REALITYの配信基盤で採用されている WebSocket + Protocol Buffers の組み合わせを技術的な参考にしつつ、初期スコープをテキストチャットに限定します。REALITYの非公開APIや独自通信仕様を再実装するものではありません。

> [!NOTE]
> **Status: Planning / Experimental**  
> 本READMEは初期設計です。機能、ディレクトリ、コマンドは実装予定を含みます。Google公式Rust Protobufはベータ段階であり、APIやビルド手順の変更を想定します。

Goals

1. Official Rust Protobuf: Google公式のRustランタイムとコード生成器を使用し、生成コードの操作性やビルド要件を検証する。
2. Schema-first: .protoを通信仕様の唯一の定義元とし、RustとTypeScriptの型を生成する。
3. Real-time communication: WebSocketのBinary MessageにProtobufバイト列を格納して双方向通信する。
4. Cross-language compatibility: Rustとブラウザ間で同一メッセージをエンコード・デコードできることをテストする。
5. Measurement: 同じ論理データについてJSONとProtobufのサイズ、処理時間を比較する。

MVP Scope

In scope

• ニックネームを入力して単一の公開チャットルームに参加する
• テキストメッセージを送受信する
• 接続中の全クライアントへメッセージをブロードキャストする
• 入室・退出を通知する
• 接続状態を表示し、切断時に再接続を試みる
• 不正メッセージ、過大なペイロード、無効な入力を拒否する
• 複数ブラウザ間で送受信できることを確認する

Out of scope

• アカウント認証、アクセス制御、永続化、過去ログ
• 複数ルーム、ダイレクトメッセージ、画像・ファイル送信
• アバター、音声・映像配信、3Dレンダリング
• Redis、メッセージブローカー、マルチサーバー構成
• 本番環境に必要な包括的な不正利用対策や運用基盤

Tech Stack

|Layer           |Technology                                  |Responsibility     |
|----------------|--------------------------------------------|-------------------|
|Web UI          |React, TypeScript, Vite                     |チャット画面・接続状態        |
|Browser Protobuf|`@bufbuild/protobuf`                        |TypeScript側のメッセージ変換|
|Transport       |WebSocket                                   |双方向通信・メッセージ境界      |
|Backend         |Rust, Axum, Tokio                           |WebSocket接続・検証・配信  |
|Rust Protobuf   |Google公式 `protobuf` v4（`google-protobuf`ベース）|Rust側のメッセージ変換      |
|Rust Codegen    |Google公式Rust Protobufコード生成器                 |`.proto`からRustコード生成|
|Schema / Lint   |Protocol Buffers (`proto3`), Buf            |共通スキーマ・互換性チェック     |

採用方針: prostではなくGoogle公式実装を優先して評価します。公式実装はPure Rustではなく、C/C++系カーネルを利用します。protocはランタイムと互換のある厳密なバージョンを使い、依存を固定します。実際のクレート名・バージョン・セットアップコマンドはPhase 1で検証してから確定します。

Architecture

flowchart TB
    Schema["Shared .proto schema"]
    subgraph Client["Web Client / React"]
        UI["Chat UI"] <--> TS["TypeScript Protobuf"]
    end
    subgraph Server["Rust Server / Axum"]
        WS["WebSocket Handler"] <--> RS["Official Rust Protobuf"]
        RS <--> Room["In-memory Room / Broadcast"]
    end
    Schema -->|TypeScript codegen| TS
    Schema -->|Rust codegen| RS
    TS <-->|WebSocket binary messages| WS

Message flow

sequenceDiagram
    participant A as React Client A
    participant S as Rust Server
    participant B as React Client B
    A->>S: WebSocket connect
    B->>S: WebSocket connect
    A->>S: ClientEnvelope(join) [binary]
    S-->>A: ServerEnvelope(joined) [binary]
    A->>S: ClientEnvelope(send_chat) [binary]
    Note over S: Decode → validate → assign sender/time/id → encode
    S-->>A: ServerEnvelope(chat) [binary]
    S-->>B: ServerEnvelope(chat) [binary]

Protocol

• 1 WebSocket Binary Messageには、1つのProtobuf envelopeを格納します。独自の長さプレフィックスは設けません。
• クライアント送信とサーバー通知は異なるメッセージ型にし、イベント種別はoneofで表現します。
• テキストフレームはアプリケーションメッセージとして扱いません。
• クライアントは送信者IDやサーバー時刻を決めません。サーバーが割り当てます。
• メッセージ長、ニックネーム長、空文字、不正なProtobuf、参加前送信を検証します。
• フィールド番号を再利用せず、削除時にはreservedを使います。互換性チェックにはBufを使用します。

Draft schema

syntax = "proto3";

package protolive.v1;

message JoinRequest {
  string nickname = 1;
}

message SendChatRequest {
  string content = 1;
}

message ClientEnvelope {
  oneof payload {
    JoinRequest join = 1;
    SendChatRequest send_chat = 2;
  }
}

message ChatMessage {
  string id = 1;
  string sender_id = 2;
  string nickname = 3;
  string content = 4;
  uint64 timestamp_ms = 5;
}

message MemberEvent {
  string sender_id = 1;
  string nickname = 2;
}

message ErrorEvent {
  string code = 1;
  string message = 2;
}

message ServerEnvelope {
  oneof payload {
    MemberEvent joined = 1;
    MemberEvent left = 2;
    ChatMessage chat = 3;
    ErrorEvent error = 4;
  }
}

> このスキーマは草案です。`sender_id`は接続単位の一時識別子を想定し、認証済みユーザーIDではありません。切断・再接続時の通知重複や未達は、MVPでは完全保証しません。

Planned Repository Structure

protolive/
├── README.md
├── Cargo.toml                 # Cargo workspace (planned)
├── pnpm-workspace.yaml       # pnpm workspace (planned)
├── proto/
│   └── protolive/v1/chat.proto
├── apps/
│   ├── server/               # Rust, Axum, Tokio
│   │   ├── Cargo.toml
│   │   ├── build.rs
│   │   └── src/
│   └── web/                  # React, TypeScript, Vite
├── packages/
│   └── protocol-ts/          # Generated TypeScript bindings
├── benchmarks/
└── docs/

Rustの生成コードはサーバーのビルド過程で生成する方針です。生成物のコミット有無はPhase 1で決定します。

Roadmap

Phase 1: Official Rust Protobuf foundation

☐ Google公式ランタイム・コード生成器・対応protocを固定
☐ .protoの作成とRust/TypeScriptコード生成
☐ 各言語でエンコード・デコードの往復テスト
☐ Rust → TypeScript、TypeScript → Rustの相互運用テスト
☐ ビルド方法、APIの制約、既知の問題を記録

Phase 2: Rust chat server

☐ AxumのWebSocketエンドポイント
☐ 参加・切断・配信のインメモリ状態管理
☐ Protobufデコード、入力検証、エラー処理
☐ 複数接続へのブロードキャスト

Phase 3: React chat client

☐ ニックネーム入力・メッセージ入力・ログ表示
☐ Protobufバイナリの送受信
☐ 接続状態表示・再接続時の再参加
☐ 2つ以上のブラウザによるE2E確認

Phase 4: Evaluation

☐ JSONとProtobufのペイロードサイズ比較
☐ シリアライズ・デシリアライズ時間比較
☐ 接続数や送信頻度を変えた負荷実験
☐ Google公式Rust Protobufの採用評価を文書化

MVP Acceptance Criteria

1. 2つのブラウザでそれぞれニックネームを設定し、同じチャットへ接続できる。
2. Client AのメッセージがProtobufのWebSocket Binary MessageとしてRustサーバーに届く。
3. サーバーはデコード、検証、送信者情報と時刻の付与を行って、全接続先へ再エンコードして配信する。
4. Client Bが正しい送信者・内容・時刻のメッセージを画面に表示できる。
5. 不正なバイナリや規定外の入力がサーバープロセスを停止させない。
6. RustとTypeScriptの相互運用テストが通る。

Performance Notes

測定はシリアライズ単体とWebSocketを含めたエンドツーエンドを分けて行います。Protobufは常にJSONより小さい・高速とは限らないため、結果は測定条件とともに公開します。

|Metric            |Measurement |
|------------------|------------|
|Payload size      |バイト数        |
|Encode / decode   |処理時間        |
|Throughput        |メッセージ数 / 秒  |
|End-to-end latency|クライアント間の到達時間|
|Memory usage      |サーバーのメモリ消費  |

References

• Google Protocol Buffers / Rust Reference
• Google Protocol Buffers / Building Rust Protos
• Google Protocol Buffers GitHub
• REALITY: 配信サーバをクラウド化する実験をしてみた
• REALITY: 通信速度改善のためのprotobuf対応
• Axum
• Buf

License

TBD. 公開前にライセンスを選定します。