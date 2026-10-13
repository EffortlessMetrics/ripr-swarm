import { expect } from "chai";
import { parseTitle } from "../src/title";

describe("parseTitle", () => {
  it("rejects a blank title with its message", () => {
    expect(() => parseTitle("  ")).to.throw("title must not be blank");
  });
});
