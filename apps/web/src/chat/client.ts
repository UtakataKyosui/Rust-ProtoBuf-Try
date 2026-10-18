// ブラウザ標準 WebSocket + Protobuf-ES によるチャットクライアント(UI 非依存)。
// 1 Binary フレーム = 1 Envelope。切断時は指数バックオフで再接続し、接続後に自動で再参加する。
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
  ClientEnvelopeSchema,
  JoinRequestSchema,
  SendChatRequestSchema,
  ServerEnvelopeSchema,
  type ServerEnvelope,
} from "@protolive/protocol-ts";

export type ConnectionStatus = "idle" | "connecting" | "open" | "reconnecting";

export type ClientEvent =
  | { type: "status"; status: ConnectionStatus }
  | { type: "server"; envelope: ServerEnvelope }
  | { type: "decode_error"; error: unknown };

/** テストで差し替えられるよう、WebSocket のうち使う部分だけを型にする。 */
export interface SocketLike {
  binaryType: string;
  onopen: ((ev: unknown) => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  onclose: ((ev: unknown) => void) | null;
  onerror: ((ev: unknown) => void) | null;
  send(data: Uint8Array): void;
  close(code?: number, reason?: string): void;
}

export interface BackoffOptions {
  baseMs: number;
  maxMs: number;
}

export const DEFAULT_BACKOFF: BackoffOptions = { baseMs: 500, maxMs: 10_000 };

/** attempt(0 始まり)回目の再接続待ち時間。base * 2^attempt を max で頭打ちにする。 */
export function backoffDelay(attempt: number, { baseMs, maxMs }: BackoffOptions): number {
  return Math.min(maxMs, baseMs * 2 ** Math.min(attempt, 30));
}

export interface ChatClientOptions {
  url: string;
  createSocket?: (url: string) => SocketLike;
  backoff?: BackoffOptions;
  setTimer?: (fn: () => void, ms: number) => unknown;
  clearTimer?: (handle: unknown) => void;
}

type Listener = (event: ClientEvent) => void;

export class ChatClient {
  private readonly url: string;
  private readonly createSocket: (url: string) => SocketLike;
  private readonly backoff: BackoffOptions;
  private readonly setTimer: (fn: () => void, ms: number) => unknown;
  private readonly clearTimer: (handle: unknown) => void;
  private readonly listeners = new Set<Listener>();

  private socket: SocketLike | null = null;
  private nickname: string | null = null;
  private attempt = 0;
  private retryTimer: unknown = null;
  private status: ConnectionStatus = "idle";

  constructor(options: ChatClientOptions) {
    this.url = options.url;
    this.createSocket =
      options.createSocket ?? ((url) => new WebSocket(url) as unknown as SocketLike);
    this.backoff = options.backoff ?? DEFAULT_BACKOFF;
    this.setTimer = options.setTimer ?? ((fn, ms) => setTimeout(fn, ms));
    this.clearTimer = options.clearTimer ?? ((h) => clearTimeout(h as number));
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  getStatus(): ConnectionStatus {
    return this.status;
  }

  /** 接続して nickname で参加する。以後、切断されても自動で再接続・再参加する。 */
  connect(nickname: string): void {
    this.nickname = nickname;
    this.attempt = 0;
    this.cancelRetry();
    this.detachSocket();
    this.open("connecting");
  }

  /** ユーザー操作による切断。再接続はしない。 */
  disconnect(): void {
    this.nickname = null;
    this.cancelRetry();
    this.detachSocket();
    this.setStatus("idle");
  }

  /** 発言を送る。接続中でなければ false を返す(キューには積まない)。 */
  sendChat(content: string): boolean {
    if (this.status !== "open" || !this.socket) return false;
    const envelope = create(ClientEnvelopeSchema, {
      payload: { case: "sendChat", value: create(SendChatRequestSchema, { content }) },
    });
    this.socket.send(toBinary(ClientEnvelopeSchema, envelope));
    return true;
  }

  private open(status: ConnectionStatus): void {
    this.setStatus(status);
    const socket = this.createSocket(this.url);
    socket.binaryType = "arraybuffer";
    this.socket = socket;

    socket.onopen = () => {
      if (this.socket !== socket || this.nickname === null) return;
      this.attempt = 0;
      this.setStatus("open");
      const join = create(ClientEnvelopeSchema, {
        payload: {
          case: "join",
          value: create(JoinRequestSchema, { nickname: this.nickname }),
        },
      });
      socket.send(toBinary(ClientEnvelopeSchema, join));
    };

    socket.onmessage = (ev) => {
      if (this.socket !== socket) return;
      // Text フレームはアプリケーションメッセージとして扱わない。
      if (!(ev.data instanceof ArrayBuffer)) return;
      try {
        const envelope = fromBinary(ServerEnvelopeSchema, new Uint8Array(ev.data));
        this.emit({ type: "server", envelope });
      } catch (error) {
        this.emit({ type: "decode_error", error });
      }
    };

    socket.onclose = () => {
      if (this.socket !== socket) return;
      this.socket = null;
      this.scheduleReconnect();
    };
    // error の後には必ず close が来るため、再接続は onclose に一本化する。
    socket.onerror = () => {};
  }

  private scheduleReconnect(): void {
    if (this.nickname === null) {
      this.setStatus("idle");
      return;
    }
    this.setStatus("reconnecting");
    const delay = backoffDelay(this.attempt, this.backoff);
    this.attempt += 1;
    this.retryTimer = this.setTimer(() => {
      this.retryTimer = null;
      this.open("reconnecting");
    }, delay);
  }

  private cancelRetry(): void {
    if (this.retryTimer !== null) {
      this.clearTimer(this.retryTimer);
      this.retryTimer = null;
    }
  }

  /** 旧ソケットのイベントを無視して閉じる。 */
  private detachSocket(): void {
    const socket = this.socket;
    this.socket = null;
    if (socket) {
      socket.onopen = socket.onmessage = socket.onclose = socket.onerror = null;
      socket.close(1000);
    }
  }

  private setStatus(status: ConnectionStatus): void {
    if (this.status === status) return;
    this.status = status;
    this.emit({ type: "status", status });
  }

  private emit(event: ClientEvent): void {
    for (const listener of this.listeners) listener(event);
  }
}
