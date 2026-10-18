// Rust 側 (apps/server/tests/golden.rs) と共有する golden バイナリの検証。
// fixtures/golden/*.bin が正本で、同じメッセージを TS で構築したエンコード結果との一致と、
// fixture をデコードした内容の一致を確認する。
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { describe, expect, it } from "vite-plus/test";
import {
  ChatMessageSchema,
  ClientEnvelopeSchema,
  ErrorEventSchema,
  JoinRequestSchema,
  MemberEventSchema,
  SendChatRequestSchema,
  ServerEnvelopeSchema,
} from "./index.js";

function fixture(name: string): Uint8Array {
  const url = new URL(`../../../fixtures/golden/${name}.bin`, import.meta.url);
  return new Uint8Array(readFileSync(fileURLToPath(url)));
}

const join = (nickname: string) =>
  create(ClientEnvelopeSchema, {
    payload: { case: "join", value: create(JoinRequestSchema, { nickname }) },
  });

const sendChat = (content: string) =>
  create(ClientEnvelopeSchema, {
    payload: { case: "sendChat", value: create(SendChatRequestSchema, { content }) },
  });

const memberEvent = (kind: "joined" | "left") =>
  create(ServerEnvelopeSchema, {
    payload: {
      case: kind,
      value: create(MemberEventSchema, { senderId: "conn-1", nickname: "alice" }),
    },
  });

describe("golden fixtures: TS encode matches Rust bytes", () => {
  const clientCases: [string, ReturnType<typeof join>][] = [
    ["client_join", join("alice")],
    ["client_join_japanese", join("ゆうき🎉")],
    ["client_send_chat_japanese", sendChat("こんにちは、世界")],
    ["client_send_chat_empty", sendChat("")],
    ["client_send_chat_len_127", sendChat("a".repeat(127))],
    ["client_send_chat_len_128", sendChat("a".repeat(128))],
    ["client_send_chat_len_16384", sendChat("a".repeat(16384))],
  ];

  it.each(clientCases)("%s", (name, message) => {
    const bytes = fixture(name);
    expect(toBinary(ClientEnvelopeSchema, message)).toEqual(bytes);
    expect(fromBinary(ClientEnvelopeSchema, bytes)).toEqual(message);
  });

  const serverCases: [string, ReturnType<typeof memberEvent>][] = [
    ["server_joined", memberEvent("joined")],
    ["server_left", memberEvent("left")],
    [
      "server_chat",
      create(ServerEnvelopeSchema, {
        payload: {
          case: "chat",
          value: create(ChatMessageSchema, {
            id: "msg-0001",
            senderId: "conn-1",
            nickname: "ゆうき",
            content: "こんにちは 👋",
            timestampMs: 1_760_000_000_123n,
          }),
        },
      }),
    ],
    [
      "server_error",
      create(ServerEnvelopeSchema, {
        payload: {
          case: "error",
          value: create(ErrorEventSchema, {
            code: "invalid_nickname",
            message: "ニックネームが不正です",
          }),
        },
      }),
    ],
  ];

  it.each(serverCases)("%s", (name, message) => {
    const bytes = fixture(name);
    expect(toBinary(ServerEnvelopeSchema, message)).toEqual(bytes);
    expect(fromBinary(ServerEnvelopeSchema, bytes)).toEqual(message);
  });
});

describe("oneof", () => {
  it("joined と left は同じ MemberEvent でも case で区別される", () => {
    const decoded = fromBinary(ServerEnvelopeSchema, fixture("server_left"));
    expect(decoded.payload.case).toBe("left");
  });

  it("空の send_chat も case が保持される", () => {
    const decoded = fromBinary(ClientEnvelopeSchema, fixture("client_send_chat_empty"));
    expect(decoded.payload.case).toBe("sendChat");
  });
});

describe("unknown fields", () => {
  it("未知フィールドを無視して既知フィールドを読み、再エンコードで保持する", () => {
    const bytes = fixture("client_join_unknown_field");
    const decoded = fromBinary(ClientEnvelopeSchema, bytes);
    expect(decoded.payload.case).toBe("join");
    if (decoded.payload.case === "join") {
      expect(decoded.payload.value.nickname).toBe("alice");
    }
    expect(toBinary(ClientEnvelopeSchema, decoded)).toEqual(bytes);
  });
});

describe("invalid bytes", () => {
  it.each(["invalid_truncated", "invalid_wire_type", "invalid_utf8"])("%s は拒否される", (name) => {
    expect(() => fromBinary(ClientEnvelopeSchema, fixture(name))).toThrow();
  });
});
