import { expect, it } from "vitest";
import { loadOrDefault } from "../src/loadOrDefault";

it("returns what the reader resolves", async () => {
  await expect(loadOrDefault(async () => 42, 0)).resolves.toBe(42);
});
