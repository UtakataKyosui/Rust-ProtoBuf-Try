import react from "@vitejs/plugin-react";
import { defineConfig } from "vite-plus";

// 開発・E2E 時はブラウザと同一オリジンの /ws を Rust サーバーへ転送する。
const wsTarget = process.env.PROTOLIVE_WS_TARGET ?? "ws://127.0.0.1:3000";

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: { "/ws": { target: wsTarget, ws: true } },
  },
  lint: {
    options: { typeAware: true, typeCheck: true },
  },
});
