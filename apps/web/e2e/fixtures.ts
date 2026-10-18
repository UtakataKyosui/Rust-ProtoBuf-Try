import { spawn, type ChildProcess } from "node:child_process";
import { fileURLToPath } from "node:url";
import { test as base, expect, type Page } from "@playwright/test";
import { SERVER_PORT } from "../playwright.config";

const SERVER_BIN =
  process.env.PROTOLIVE_SERVER_BIN ??
  fileURLToPath(new URL("../../../target/debug/protolive-server", import.meta.url));

/** テスト内から停止・再起動できる Rust サーバーのプロセス。 */
export class ServerProcess {
  private child: ChildProcess | null = null;

  async start(): Promise<void> {
    const child = spawn(SERVER_BIN, [], {
      env: { ...process.env, PROTOLIVE_ADDR: `127.0.0.1:${SERVER_PORT}`, RUST_LOG: "warn" },
      stdio: "inherit",
    });
    this.child = child;
    const exited = new Promise<never>((_, reject) =>
      child.once("exit", (code) => reject(new Error(`server exited early (code ${code})`))),
    );
    await Promise.race([waitForPort(), exited]);
  }

  async stop(): Promise<void> {
    const child = this.child;
    this.child = null;
    if (!child || child.exitCode !== null) return;
    const done = new Promise<void>((resolve) => child.once("exit", () => resolve()));
    child.kill("SIGKILL"); // 異常終了(クラッシュ)相当。graceful 経路は Rust 側テストで検証済み。
    await done;
  }

  async restart(): Promise<void> {
    await this.stop();
    await this.start();
  }
}

async function waitForPort(): Promise<void> {
  const deadline = Date.now() + 10_000;
  for (;;) {
    try {
      const res = await fetch(`http://127.0.0.1:${SERVER_PORT}/ws`);
      // WebSocket 以外のリクエストには 4xx が返る。応答があれば待ち受け中。
      void res.status;
      return;
    } catch {
      if (Date.now() > deadline) throw new Error("server did not start");
      await new Promise((r) => setTimeout(r, 100));
    }
  }
}

export const test = base.extend<{ server: ServerProcess }>({
  // eslint-disable-next-line no-empty-pattern -- Playwright は fixture 引数の分割代入を要求する
  server: async ({}, use) => {
    const server = new ServerProcess();
    await server.start();
    await use(server);
    await server.stop();
  },
});

export { expect };

/** 新しいブラウザコンテキスト(=別ブラウザ相当)でチャットを開き、参加する。 */
export async function joinChat(page: Page, nickname: string): Promise<void> {
  await page.goto("/");
  await page.getByLabel("ニックネーム").fill(nickname);
  await page.getByRole("button", { name: "参加" }).click();
  await expect(page.getByLabel("チャットログ")).toContainText(`${nickname} が入室しました`);
  await expect(page.getByTestId("status")).toHaveAttribute("data-status", "open");
}

export async function sendMessage(page: Page, text: string): Promise<void> {
  await page.getByLabel("メッセージ").fill(text);
  await page.getByRole("button", { name: "送信" }).click();
}
