// チャット画面のローカル状態。副作用を持たない reducer として UI/通信から分離する。
import type { ServerEnvelope } from "@protolive/protocol-ts";
import type { ConnectionStatus } from "./client";

/** メモリを使い切らないよう、画面に保持するログの上限。 */
export const MAX_LOG_ENTRIES = 500;

export type LogEntry =
  | {
      kind: "chat";
      key: string;
      id: string;
      senderId: string;
      nickname: string;
      content: string;
      timestampMs: number;
    }
  | { kind: "joined" | "left"; key: string; senderId: string; nickname: string }
  | { kind: "error"; key: string; code: string; message: string };

export interface ChatState {
  status: ConnectionStatus;
  /** 自分の接続 ID。サーバーが返す自分の Joined で確定する(未参加なら null)。 */
  selfId: string | null;
  /** 一度でも参加に成功したか。再接続中も画面をチャット表示に保つために使う。 */
  everJoined: boolean;
  /** 接続が開いてから自分の Joined を待っている間 true。 */
  awaitingSelf: boolean;
  log: LogEntry[];
  nextKey: number;
}

export const initialState: ChatState = {
  status: "idle",
  selfId: null,
  everJoined: false,
  awaitingSelf: false,
  log: [],
  nextKey: 0,
};

export type ChatAction =
  | { type: "status"; status: ConnectionStatus }
  | { type: "server"; envelope: ServerEnvelope }
  | { type: "decode_error" };

function append(state: ChatState, entry: (key: string) => LogEntry): ChatState {
  const log = [...state.log, entry(`log-${state.nextKey}`)];
  return { ...state, log: log.slice(-MAX_LOG_ENTRIES), nextKey: state.nextKey + 1 };
}

export function chatReducer(state: ChatState, action: ChatAction): ChatState {
  switch (action.type) {
    case "status":
      // 接続のたびに接続 ID が変わるため、open で参加状態をリセットして再参加を待つ。
      // idle(ユーザーによる退出)ではログも含めて初期状態へ戻す。
      if (action.status === "idle") return initialState;
      // 新規の接続(参加操作)では前回の失敗ログなどを持ち越さない。
      if (action.status === "connecting") return { ...initialState, status: "connecting" };
      return {
        ...state,
        status: action.status,
        selfId: null,
        awaitingSelf: action.status === "open",
      };
    case "decode_error":
      return append(state, (key) => ({
        kind: "error",
        key,
        code: "decode_error",
        message: "サーバーからの不正なメッセージを破棄しました",
      }));
    case "server": {
      const payload = action.envelope.payload;
      switch (payload.case) {
        case "chat": {
          const m = payload.value;
          return append(state, (key) => ({
            kind: "chat",
            key,
            id: m.id,
            senderId: m.senderId,
            nickname: m.nickname,
            content: m.content,
            timestampMs: Number(m.timestampMs),
          }));
        }
        case "joined":
        case "left": {
          const kind = payload.case;
          const { senderId, nickname } = payload.value;
          const next = append(state, (key) => ({ kind, key, senderId, nickname }));
          return kind === "joined" && state.awaitingSelf
            ? { ...next, selfId: senderId, everJoined: true, awaitingSelf: false }
            : next;
        }
        case "error": {
          const { code, message } = payload.value;
          return append(state, (key) => ({ kind: "error", key, code, message }));
        }
        default:
          return state;
      }
    }
  }
}
