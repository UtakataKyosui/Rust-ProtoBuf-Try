import { renderToString } from "react-dom/server";
import { expect, test } from "vite-plus/test";
import { App, LogItem, StatusBadge } from "./App";

// renderToString は effect を実行しないため、WebSocket を使わず初期表示のみ検証できる。
// window.location は node 環境にないので useChat の既定 URL 計算用にスタブする。
globalThis.window = { location: { protocol: "http:", host: "localhost" } } as unknown as Window &
  typeof globalThis;

test("初期表示はニックネーム入力と未接続バッジ", () => {
  const html = renderToString(<App />);
  expect(html).toContain("<h1>ProtoLive</h1>");
  expect(html).toContain("ニックネーム");
  expect(html).toContain("未接続");
  expect(html).not.toContain("チャットログ");
});

test("StatusBadge は状態ごとのラベルを表示する", () => {
  expect(renderToString(<StatusBadge status="reconnecting" />)).toContain("再接続中");
});

test("LogItem は発言・入退室・エラーを描画し、本文は HTML エスケープされる", () => {
  const chat = renderToString(
    <LogItem
      selfId="me"
      entry={{
        kind: "chat",
        key: "k",
        id: "1",
        senderId: "me",
        nickname: "alice",
        content: "<b>hi</b>",
        timestampMs: 0,
      }}
    />,
  );
  expect(chat).toContain("mine");
  expect(chat).toContain("&lt;b&gt;hi&lt;/b&gt;");
  // renderToString は隣接テキストの間に <!-- --> を挟むので除去して比較する。
  const left = renderToString(
    <LogItem selfId={null} entry={{ kind: "left", key: "k", senderId: "x", nickname: "bob" }} />,
  ).replaceAll("<!-- -->", "");
  expect(left).toContain("bob が退室しました");
});
