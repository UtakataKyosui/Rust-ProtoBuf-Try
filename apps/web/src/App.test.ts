import { expect, test } from "vite-plus/test";
import { App } from "./App";

test("App is a component", () => {
  expect(typeof App).toBe("function");
});
