import { mock, test, expect } from "bun:test";
mock.module("server-only", () => ({}));
const { safeReturnTo } = await import("../apps/app/lib/api");

test("return URLs stay on the app origin after browser normalization", () => {
  for (const value of [
    "//evil.example",
    "/\\evil.example",
    "/\n/evil.example",
    "/\t/evil.example",
    "https://evil.example",
    null,
  ]) {
    expect(safeReturnTo(value)).toBe("/dashboard");
  }
  expect(safeReturnTo("/oauth/consent?request_id=123")).toBe(
    "/oauth/consent?request_id=123",
  );
});
