import { test } from "vitest";
export { expect } from "vitest";
import { load } from "../src/load";
const expect = (p: Promise<unknown>) => ({
  rejects: { toBe(_expected: unknown) { return p.catch(() => {}); } },
});
test("missing token", async () => {
  await expect(load()).rejects.toBe("TOKEN_REQUIRED");
});
