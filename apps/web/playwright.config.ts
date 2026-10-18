import { defineConfig } from "@playwright/test";

// E2E 用ポート。開発用の 3000 / 5173 と衝突しないよう固定の別ポートを使う。
export const SERVER_PORT = 3100;
const WEB_PORT = 5174;

export default defineConfig({
  testDir: "./e2e",
  testMatch: "**/*.e2e.ts",
  // Rust サーバーのポートが 1 つなので直列実行する。
  workers: 1,
  fullyParallel: false,
  timeout: 60_000,
  expect: { timeout: 15_000 },
  reporter: process.env.CI ? [["github"], ["list"]] : "list",
  retries: 0,
  use: {
    baseURL: `http://127.0.0.1:${WEB_PORT}`,
    trace: "retain-on-failure",
  },
  webServer: {
    command: `pnpm exec vp dev --host 127.0.0.1 --port ${WEB_PORT} --strictPort`,
    url: `http://127.0.0.1:${WEB_PORT}`,
    reuseExistingServer: !process.env.CI,
    env: { PROTOLIVE_WS_TARGET: `ws://127.0.0.1:${SERVER_PORT}` },
  },
});
