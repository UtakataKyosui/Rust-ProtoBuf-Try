import { renderToString } from "react-dom/server";
import { expect, test } from "vite-plus/test";
import { App } from "./App";

test("App renders the ProtoLive heading", () => {
  expect(renderToString(<App />)).toContain("<h1>ProtoLive</h1>");
});
