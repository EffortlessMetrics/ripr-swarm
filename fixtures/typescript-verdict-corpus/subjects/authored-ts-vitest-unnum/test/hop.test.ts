import { expect, test } from "vitest";
import { isHopNetworkError } from "../src/hop/index";

test("a reset is a network error through two re-exports", () => {
  expect(isHopNetworkError({ code: "ECONNRESET" })).toBe(true);
  expect(isHopNetworkError({ code: "EACCES" })).toBe(false);
});
