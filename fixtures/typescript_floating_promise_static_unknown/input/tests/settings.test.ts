import { test, expect } from "vitest";
import { f } from "../src/settings";

test("t", async () => {
  expect(Promise.resolve(f(100))).resolves.toBe(false);
});
