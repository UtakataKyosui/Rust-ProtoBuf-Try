// README の MVP 受け入れ条件(2 ブラウザ送受信・不正入力・再接続・サーバー再起動)を自動検証する。
import { expect, joinChat, sendMessage, test } from "./fixtures";

const status = (page: import("@playwright/test").Page) => page.getByTestId("status");

test("2 つのブラウザが同じルームで送受信でき、入退室が通知される", async ({
  browser,
  server: _server,
}) => {
  const alice = await (await browser.newContext()).newPage();
  const bob = await (await browser.newContext()).newPage();

  await joinChat(alice, "alice");
  await joinChat(bob, "ボブ");
  await expect(alice.getByLabel("チャットログ")).toContainText("ボブ が入室しました");

  await sendMessage(alice, "こんにちは 👋");
  for (const page of [alice, bob]) {
    const log = page.getByLabel("チャットログ");
    await expect(log).toContainText("alice");
    await expect(log).toContainText("こんにちは 👋");
  }

  await sendMessage(bob, "hello from bob");
  await expect(alice.getByLabel("チャットログ")).toContainText("hello from bob");
  // 送信後は入力欄がクリアされる
  await expect(bob.getByLabel("メッセージ")).toHaveValue("");

  await bob.context().close();
  await expect(alice.getByLabel("チャットログ")).toContainText("ボブ が退室しました");
});

test("参加前の他人のログは見えず、後から参加した人に過去ログは届かない", async ({
  browser,
  server: _server,
}) => {
  const alice = await (await browser.newContext()).newPage();
  await joinChat(alice, "alice");
  await sendMessage(alice, "past message");
  await expect(alice.getByLabel("チャットログ")).toContainText("past message");

  const bob = await (await browser.newContext()).newPage();
  await joinChat(bob, "bob");
  await expect(bob.getByLabel("チャットログ")).not.toContainText("past message");
});

test("不正な入力はエラー表示され、接続は維持される", async ({ page, browser, server: _server }) => {
  await page.goto("/");
  // 空のニックネームでは参加できない
  await expect(page.getByRole("button", { name: "参加" })).toBeDisabled();

  // 32 文字超のニックネームはサーバーが拒否し、フォームに留まる
  await page.getByLabel("ニックネーム").fill("a".repeat(33));
  await page.getByRole("button", { name: "参加" }).click();
  await expect(page.getByRole("alert")).toBeVisible();
  await expect(page.getByLabel("ニックネーム")).toBeVisible();

  await page.getByLabel("ニックネーム").fill("alice");
  await page.getByRole("button", { name: "参加" }).click();
  await expect(page.getByLabel("チャットログ")).toContainText("alice が入室しました");

  // 1000 文字超の本文は拒否され、エラーが出ても接続は維持される
  const other = await (await browser.newContext()).newPage();
  await joinChat(other, "bob");
  await sendMessage(page, "x".repeat(1001));
  await expect(page.getByLabel("チャットログ").getByRole("alert")).toBeVisible();
  await expect(status(page)).toHaveAttribute("data-status", "open");
  await expect(other.getByLabel("チャットログ")).not.toContainText("xxxxxxxxxx");

  // 空白のみは送信ボタンが無効
  await page.getByLabel("メッセージ").fill("   ");
  await expect(page.getByRole("button", { name: "送信" })).toBeDisabled();

  await sendMessage(page, "recovered");
  await expect(other.getByLabel("チャットログ")).toContainText("recovered");
});

test("サーバー再起動後に自動で再接続・再参加し、送受信を再開できる", async ({
  browser,
  server,
}) => {
  const alice = await (await browser.newContext()).newPage();
  const bob = await (await browser.newContext()).newPage();
  await joinChat(alice, "alice");
  await joinChat(bob, "bob");

  await server.stop();
  await expect(status(alice)).toHaveAttribute("data-status", "reconnecting");
  await expect(status(bob)).toHaveAttribute("data-status", "reconnecting");
  // 切断中は送信できない
  await expect(alice.getByRole("button", { name: "送信" })).toBeDisabled();

  await server.start();
  await expect(status(alice)).toHaveAttribute("data-status", "open", { timeout: 30_000 });
  await expect(status(bob)).toHaveAttribute("data-status", "open", { timeout: 30_000 });

  // 再接続後は同じニックネームで再参加済みで、互いに送受信できる
  await expect(alice.getByLabel("メッセージ")).toBeEnabled();
  await expect(bob.getByLabel("メッセージ")).toBeEnabled();
  await sendMessage(alice, "after restart");
  await expect(bob.getByLabel("チャットログ")).toContainText("after restart");
  await sendMessage(bob, "ack");
  await expect(alice.getByLabel("チャットログ")).toContainText("ack");
});

test("退出するとフォームに戻り、再参加できる", async ({ page, server: _server }) => {
  await joinChat(page, "alice");
  await page.getByRole("button", { name: "退出" }).click();
  await expect(status(page)).toHaveAttribute("data-status", "idle");
  await expect(page.getByLabel("ニックネーム")).toBeVisible();

  await page.getByLabel("ニックネーム").fill("alice2");
  await page.getByRole("button", { name: "参加" }).click();
  await expect(page.getByLabel("チャットログ")).toContainText("alice2 が入室しました");
});
