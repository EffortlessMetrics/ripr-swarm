import test from "node:test";
import assert from "node:assert";
import { chargeEx35 } from "../src/chargeEx35";

test("chargeEx35", async () => {
  await assert.rejects(chargeEx35(-1), { message: "charge must be nonnegative" });
});
