import { expect, test } from "vitest";
import { isRawNetworkError } from "../src/collide/index";

test("a timeout is a raw network error", () => {
  expect(isRawNetworkError({ code: "ETIMEDOUT" })).toBe(true);
});

test("a reset is not the barrel's raw network error", () => {
  expect(isRawNetworkError({ code: "ECONNRESET" })).toBe(false);
});
