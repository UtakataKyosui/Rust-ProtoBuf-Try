import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
  ChatMessageSchema,
  ClientEnvelopeSchema,
  MemberEventSchema,
  ServerEnvelopeSchema,
} from "@protolive/protocol-ts";
import { describe, expect, it } from "vite-plus/test";
import { backoffDelay, ChatClient, type ClientEvent, type SocketLike } from "./client";

class FakeSocket implements SocketLike {
  binaryType = "blob";
  onopen: SocketLike["onopen"] = null;
  onmessage: SocketLike["onmessage"] = null;
  onclose: SocketLike["onclose"] = null;
  onerror: SocketLike["onerror"] = null;
  sent: Uint8Array[] = [];
  closed = false;

  send(data: Uint8Array) {
    this.sent.push(data);
  }
  close() {
    this.closed = true;
  }
  open() {
    this.onopen?.({});
  }
  receive(data: unknown) {
    this.onmessage?.({ data });
  }
  drop() {
    this.onclose?.({});
  }
}

function setup() {
  const sockets: FakeSocket[] = [];
  const timers: { fn: () => void; ms: number; cleared: boolean }[] = [];
  const client = new ChatClient({
    url: "ws://test/ws",
    createSocket: () => {
      const s = new FakeSocket();
      sockets.push(s);
      return s;
    },
    backoff: { baseMs: 100, maxMs: 400 },
    setTimer: (fn, ms) => {
      const t = { fn, ms, cleared: false };
      timers.push(t);
      return t;
    },
    clearTimer: (h) => {
      (h as { cleared: boolean }).cleared = true;
    },
  });
  const events: ClientEvent[] = [];
  client.subscribe((e) => events.push(e));
  const statuses = () => events.flatMap((e) => (e.type === "status" ? [e.status] : []));
  const decode = (bytes: Uint8Array) => fromBinary(ClientEnvelopeSchema, bytes);
  return { client, sockets, timers, events, statuses, decode };
}

const arrayBufferOf = (bytes: Uint8Array): ArrayBuffer =>
  bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;

describe("backoffDelay", () => {
  it("指数的に増え、上限で頭打ちになる", () => {
    const o = { baseMs: 100, maxMs: 1000 };
    expect([0, 1, 2, 3, 4, 10].map((n) => backoffDelay(n, o))).toEqual([
      100, 200, 400, 800, 1000, 1000,
    ]);
  });
});

describe("ChatClient", () => {
  it("binaryType=arraybuffer で接続し、open 後に join を Binary で送る", () => {
    const { client, sockets, statuses, decode } = setup();
    client.connect("alice");
    expect(sockets[0]!.binaryType).toBe("arraybuffer");
    expect(statuses()).toEqual(["connecting"]);

    sockets[0]!.open();
    expect(statuses()).toEqual(["connecting", "open"]);
    const join = decode(sockets[0]!.sent[0]!);
    expect(join.payload.case).toBe("join");
    if (join.payload.case === "join") expect(join.payload.value.nickname).toBe("alice");
  });

  it("sendChat は接続中のみ送信でき、それ以外は false", () => {
    const { client, sockets, decode } = setup();
    expect(client.sendChat("x")).toBe(false);
    client.connect("alice");
    expect(client.sendChat("x")).toBe(false);
    sockets[0]!.open();
    expect(client.sendChat("こんにちは")).toBe(true);
    const sent = decode(sockets[0]!.sent[1]!);
    expect(sent.payload.case).toBe("sendChat");
    if (sent.payload.case === "sendChat") expect(sent.payload.value.content).toBe("こんにちは");
  });

  it("Binary の ServerEnvelope をデコードして通知し、不正な bytes は decode_error", () => {
    const { client, sockets, events } = setup();
    client.connect("alice");
    sockets[0]!.open();

    const chat = create(ServerEnvelopeSchema, {
      payload: {
        case: "chat",
        value: create(ChatMessageSchema, {
          id: "m1",
          senderId: "c1",
          nickname: "bob",
          content: "hi",
          timestampMs: 1n,
        }),
      },
    });
    sockets[0]!.receive(arrayBufferOf(toBinary(ServerEnvelopeSchema, chat)));
    sockets[0]!.receive(arrayBufferOf(new Uint8Array([0x0a, 0xff])));
    sockets[0]!.receive("text frame");

    const server = events.filter((e) => e.type === "server");
    expect(server).toHaveLength(1);
    expect(events.filter((e) => e.type === "decode_error")).toHaveLength(1);
  });

  it("切断されると backoff で再接続し、再接続後に同じニックネームで再参加する", () => {
    const { client, sockets, timers, statuses, decode } = setup();
    client.connect("alice");
    sockets[0]!.open();
    sockets[0]!.drop();
    expect(client.getStatus()).toBe("reconnecting");
    expect(timers.map((t) => t.ms)).toEqual([100]);

    timers[0]!.fn();
    expect(sockets).toHaveLength(2);
    sockets[1]!.drop(); // 接続できず失敗 → 待ち時間が伸びる
    expect(timers.map((t) => t.ms)).toEqual([100, 200]);

    timers[1]!.fn();
    sockets[2]!.open();
    expect(statuses().at(-1)).toBe("open");
    const rejoin = decode(sockets[2]!.sent[0]!);
    expect(rejoin.payload.case).toBe("join");
    if (rejoin.payload.case === "join") expect(rejoin.payload.value.nickname).toBe("alice");

    // 成功したので attempt がリセットされる
    sockets[2]!.drop();
    expect(timers.at(-1)!.ms).toBe(100);
  });

  it("disconnect 後は再接続せず、保留中のタイマーも取り消す", () => {
    const { client, sockets, timers } = setup();
    client.connect("alice");
    sockets[0]!.open();
    sockets[0]!.drop();
    client.disconnect();
    expect(timers[0]!.cleared).toBe(true);
    expect(client.getStatus()).toBe("idle");

    client.connect("alice");
    sockets[1]!.open();
    client.disconnect();
    expect(sockets[1]!.closed).toBe(true);
    sockets[1]!.drop(); // 無視される
    expect(timers).toHaveLength(1);
    expect(client.getStatus()).toBe("idle");
  });

  it("古いソケットのイベントは無視される", () => {
    const { client, sockets, events } = setup();
    client.connect("alice");
    client.connect("bob");
    const stale = sockets[0]!;
    const before = events.length;
    stale.open();
    stale.receive(
      arrayBufferOf(
        toBinary(
          ServerEnvelopeSchema,
          create(ServerEnvelopeSchema, {
            payload: { case: "joined", value: create(MemberEventSchema, { nickname: "x" }) },
          }),
        ),
      ),
    );
    expect(events.length).toBe(before);
    expect(stale.sent).toHaveLength(0);
  });
});
