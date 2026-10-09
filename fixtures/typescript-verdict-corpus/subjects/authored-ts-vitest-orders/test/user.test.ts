import { expect, it } from "vitest";
import { loadUser } from "../src/user";

it("rejects a non-positive id", async () => {
  await expect(loadUser(-1)).rejects.toThrow("user id must be positive");
});
