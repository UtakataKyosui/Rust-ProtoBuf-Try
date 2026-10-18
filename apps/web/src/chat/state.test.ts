import { create } from "@bufbuild/protobuf";
import {
  ChatMessageSchema,
  ErrorEventSchema,
  MemberEventSchema,
  ServerEnvelopeSchema,
  type ServerEnvelope,
} from "@protolive/protocol-ts";
import { describe, expect, it } from "vite-plus/test";
import {
  chatReducer,
  initialState,
  MAX_LOG_ENTRIES,
  type ChatAction,
  type ChatState,
} from "./state";

const joined = (senderId: string, nickname: string): ServerEnvelope =>
  create(ServerEnvelopeSchema, {
    payload: { case: "joined", value: create(MemberEventSchema, { senderId, nickname }) },
  });

const chat = (content: string): ServerEnvelope =>
  create(ServerEnvelopeSchema, {
    payload: {
      case: "chat",
      value: create(ChatMessageSchema, {
        id: "m",
        senderId: "c1",
        nickname: "alice",
        content,
        timestampMs: 1_760_000_000_000n,
      }),
    },
  });

const run = (actions: ChatAction[], from: ChatState = initialState) =>
  actions.reduce(chatReducer, from);

describe("chatReducer", () => {
  it("open 後の最初の Joined を自分の参加として扱う", () => {
    const s = run([
      { type: "status", status: "open" },
      { type: "server", envelope: joined("me", "alice") },
      { type: "server", envelope: joined("other", "bob") },
    ]);
    expect(s.selfId).toBe("me");
    expect(s.everJoined).toBe(true);
    expect(s.log.map((e) => e.kind)).toEqual(["joined", "joined"]);
  });

  it("再接続中も everJoined とログを保ち、再 open で selfId を取り直す", () => {
    const s = run([
      { type: "status", status: "open" },
      { type: "server", envelope: joined("me", "alice") },
      { type: "status", status: "reconnecting" },
    ]);
    expect(s.selfId).toBeNull();
    expect(s.everJoined).toBe(true);
    expect(s.log).toHaveLength(1);

    const again = run(
      [
        { type: "status", status: "open" },
        { type: "server", envelope: joined("me2", "alice") },
      ],
      s,
    );
    expect(again.selfId).toBe("me2");
  });

  it("退出(idle)で初期状態に戻り、新規接続で前回のログを持ち越さない", () => {
    const joinedState = run([
      { type: "status", status: "open" },
      { type: "server", envelope: joined("me", "alice") },
    ]);
    expect(chatReducer(joinedState, { type: "status", status: "idle" })).toEqual(initialState);

    const failed = run([
      { type: "status", status: "open" },
      {
        type: "server",
        envelope: create(ServerEnvelopeSchema, {
          payload: {
            case: "error",
            value: create(ErrorEventSchema, { code: "invalid_nickname", message: "bad" }),
          },
        }),
      },
    ]);
    expect(failed.log).toHaveLength(1);
    expect(chatReducer(failed, { type: "status", status: "connecting" }).log).toEqual([]);
  });

  it("ログは上限件数で切り詰められる", () => {
    const actions: ChatAction[] = Array.from({ length: MAX_LOG_ENTRIES + 20 }, (_, i) => ({
      type: "server",
      envelope: chat(`msg-${i}`),
    }));
    const s = run(actions);
    expect(s.log).toHaveLength(MAX_LOG_ENTRIES);
    const last = s.log.at(-1);
    expect(last?.kind === "chat" && last.content).toBe(`msg-${MAX_LOG_ENTRIES + 19}`);
    expect(new Set(s.log.map((e) => e.key)).size).toBe(MAX_LOG_ENTRIES);
  });

  it("decode_error はエラーログとして残る", () => {
    const s = chatReducer(initialState, { type: "decode_error" });
    expect(s.log[0]?.kind).toBe("error");
  });
});
