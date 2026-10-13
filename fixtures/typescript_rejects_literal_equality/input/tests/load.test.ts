import { expect, test } from "vitest";
import { load } from "../src/load";

test("missing token rejects with the required literal", async () => {
  await expect(load()).rejects.toBe("TOKEN_REQUIRED");
});
