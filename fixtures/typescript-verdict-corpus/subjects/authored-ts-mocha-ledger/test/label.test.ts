import { expect } from "chai";
import { parseLabel } from "../src/label";

describe("parseLabel", () => {
  it("rejects a blank label", () => {
    expect(() => parseLabel("  ")).to.throw("blank");
  });
});
