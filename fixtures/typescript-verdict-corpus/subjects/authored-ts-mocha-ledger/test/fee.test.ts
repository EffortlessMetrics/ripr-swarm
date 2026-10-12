import { assert } from "chai";
import { fee } from "../src/fee";

describe("fee", () => {
  it("charges a base plus three times the amount", () => {
    assert.strictEqual(fee(100), 330);
  });
});
