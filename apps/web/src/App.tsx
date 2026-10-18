import { useEffect, useRef, useState, type FormEvent } from "react";
import { useChat } from "./chat/useChat";
import type { ConnectionStatus } from "./chat/client";
import type { LogEntry } from "./chat/state";

const STATUS_LABEL: Record<ConnectionStatus, string> = {
  idle: "未接続",
  connecting: "接続中",
  open: "接続済み",
  reconnecting: "再接続中",
};

function formatTime(ms: number): string {
  return new Date(ms).toLocaleTimeString("ja-JP", { hour12: false });
}

export function StatusBadge({ status }: { status: ConnectionStatus }) {
  return (
    <span className={`status status-${status}`} data-testid="status" data-status={status}>
      {STATUS_LABEL[status]}
    </span>
  );
}

export function LogItem({ entry, selfId }: { entry: LogEntry; selfId: string | null }) {
  switch (entry.kind) {
    case "chat":
      return (
        <li className={`chat${entry.senderId === selfId ? " mine" : ""}`}>
          <span className="nick">{entry.nickname}</span>
          <span className="body">{entry.content}</span>
          <time>{formatTime(entry.timestampMs)}</time>
        </li>
      );
    case "joined":
      return <li className="notice">{entry.nickname} が入室しました</li>;
    case "left":
      return <li className="notice">{entry.nickname} が退室しました</li>;
    case "error":
      return (
        <li className="notice error" role="alert">
          {entry.message}
        </li>
      );
  }
}

export function App() {
  const { state, join, leave, send } = useChat();
  const [nickname, setNickname] = useState("");
  const [draft, setDraft] = useState("");
  const logEnd = useRef<HTMLLIElement>(null);

  const joined = state.selfId !== null;
  const lastEntry = state.log.at(-1);

  useEffect(() => {
    logEnd.current?.scrollIntoView?.({ block: "end" });
  }, [state.log.length]);

  const onJoin = (e: FormEvent) => {
    e.preventDefault();
    if (nickname.trim() !== "") join(nickname);
  };

  const onSend = (e: FormEvent) => {
    e.preventDefault();
    if (draft.trim() === "") return;
    // 送信できた(接続中だった)場合のみ入力欄をクリアし、切断中は下書きを残す。
    if (send(draft)) setDraft("");
  };

  return (
    <main className="app">
      <header>
        <h1>ProtoLive</h1>
        <StatusBadge status={state.status} />
      </header>

      {!state.everJoined ? (
        <form onSubmit={onJoin} className="join">
          <label>
            ニックネーム
            <input
              value={nickname}
              onChange={(e) => setNickname(e.target.value)}
              maxLength={64}
              autoFocus
            />
          </label>
          <button type="submit" disabled={nickname.trim() === ""}>
            参加
          </button>
          {lastEntry?.kind === "error" && (
            <p className="error" role="alert">
              {lastEntry.message}
            </p>
          )}
        </form>
      ) : null}

      {state.everJoined ? (
        <>
          <ul className="log" aria-label="チャットログ">
            {state.log.map((entry) => (
              <LogItem key={entry.key} entry={entry} selfId={state.selfId} />
            ))}
            <li ref={logEnd} aria-hidden="true" />
          </ul>
          <form onSubmit={onSend} className="compose">
            <input
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              aria-label="メッセージ"
              placeholder="メッセージを入力"
              disabled={!joined}
            />
            <button type="submit" disabled={!joined || draft.trim() === ""}>
              送信
            </button>
            <button type="button" onClick={leave}>
              退出
            </button>
          </form>
        </>
      ) : null}
    </main>
  );
}
