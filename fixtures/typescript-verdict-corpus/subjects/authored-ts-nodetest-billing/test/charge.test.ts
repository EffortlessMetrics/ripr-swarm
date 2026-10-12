import test from "node:test";
import assert from "node:assert/strict";
import { chargeCard } from "../src/charge";

test("a non-positive charge rejects with its message", async () => {
  await assert.rejects(chargeCard(-1), { message: "charge must be positive" });
});
