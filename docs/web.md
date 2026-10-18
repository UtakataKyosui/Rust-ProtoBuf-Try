# Web クライアント設計(React / Vite+)

`apps/web`。React 19 + TypeScript + VoidZero Vite+(`vp`)。通信は標準 `WebSocket`(`binaryType = "arraybuffer"`)と
`@bufbuild/protobuf`(Protobuf-ES)。UI ライブラリは使わず、スタイルはプレーン CSS(`src/styles.css`)。
Tailwind / shadcn/ui は MVP の規模では不要と判断して未導入(必要になった時点で追加する)。

## 構成

| ファイル | 役割 |
|---|---|
| `src/chat/client.ts` | `ChatClient`。接続・Envelope の encode/decode・指数バックオフ再接続・再参加。React 非依存 |
| `src/chat/state.ts` | 画面状態の reducer(副作用なし)。ログ上限 500 件、自分の接続 ID の判定 |
| `src/chat/useChat.ts` | `ChatClient` と reducer をつなぐ React フック。マウント時に接続オブジェクトを作り、アンマウントで破棄 |
| `src/App.tsx` | ニックネーム入力 → チャットログ・入力欄・接続状態バッジ |

状態管理は標準の `useReducer` のみ(Zustand 等は不要)。通信ロジックは UI から分離しているので、フェイク `WebSocket` で単体テストできる。

## 通信の振る舞い

- 接続先は同一オリジンの `/ws`(`ws://` / `wss://` はページのプロトコルから決定)。
  開発・E2E では Vite の `server.proxy` が Rust サーバーへ転送する(`PROTOLIVE_WS_TARGET`、既定 `ws://127.0.0.1:3000`)。
- 接続が開いたら `JoinRequest` を送り、サーバーが返す**自分の `Joined`** を最初に受けた時点で参加完了とする
  (サーバーは本人にも `Joined` を返す)。接続 ID は接続のたびに変わるため、再接続のたびに取り直す。
- 切断されると 500ms から倍々(上限 10 秒)で再接続し、成功したら同じニックネームで自動的に再参加する。
  遅い受信者として切断された場合(Close 1013)も同様に再接続する。退出ボタンによる切断では再接続しない。
- 切断中の送信はキューに積まずに拒否する(入力欄の下書きは残る)。サーバー再起動をまたぐと過去ログや未達メッセージは失われる(MVP の範囲外)。
- Text フレームは無視し、デコードできない Binary は「不正なメッセージを破棄しました」としてログに出す。
- ニックネーム不正などのサーバーエラー(`ErrorEvent`)は画面に表示し、接続は維持する。

## テスト

- 単体(`vp test run`): `ChatClient`(フェイク WebSocket、backoff、再参加、古いソケットの無視)、reducer、`renderToString` による描画。
- E2E(`pnpm --filter @protolive/web e2e`、Playwright): 実 Rust サーバーを子プロセスで起動し、Vite dev server 経由で
  2 ブラウザコンテキストの送受信・入退室通知、入力エラー、サーバー停止 → 再起動後の再接続・再参加を検証する。
  サーバーはテスト用に `127.0.0.1:3100`、Web は `5174` を使う。CI では `e2e` ジョブで実行する。
