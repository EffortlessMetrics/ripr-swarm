import { test } from "node:test";
import assert from "node:assert";
import { tierForPoints } from "../src/pricing.ts";

test("silver starts at one thousand points", () => {
  assert.equal(tierForPoints(999), "bronze");
  assert.equal(tierForPoints(1000), "silver");
});
