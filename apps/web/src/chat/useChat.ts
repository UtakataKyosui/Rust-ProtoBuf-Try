import { useCallback, useEffect, useReducer, useRef } from "react";
import { ChatClient } from "./client";
import { chatReducer, initialState } from "./state";

/** 同一オリジンの `/ws` に接続する。開発時は Vite の proxy、本番はリバースプロキシで転送する。 */
export function defaultWsUrl(loc: Pick<Location, "protocol" | "host">): string {
  return `${loc.protocol === "https:" ? "wss" : "ws"}://${loc.host}/ws`;
}

export function useChat(url: string = defaultWsUrl(window.location)) {
  const [state, dispatch] = useReducer(chatReducer, initialState);
  const clientRef = useRef<ChatClient | null>(null);

  useEffect(() => {
    const client = new ChatClient({ url });
    clientRef.current = client;
    const unsubscribe = client.subscribe((event) => {
      if (event.type === "status") dispatch({ type: "status", status: event.status });
      else if (event.type === "server") dispatch({ type: "server", envelope: event.envelope });
      else dispatch({ type: "decode_error" });
    });
    return () => {
      unsubscribe();
      client.disconnect();
      clientRef.current = null;
    };
  }, [url]);

  const join = useCallback((nickname: string) => clientRef.current?.connect(nickname), []);
  const leave = useCallback(() => clientRef.current?.disconnect(), []);
  const send = useCallback((content: string) => clientRef.current?.sendChat(content) ?? false, []);

  return { state, join, leave, send };
}
