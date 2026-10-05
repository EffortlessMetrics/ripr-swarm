import { expect } from "chai";
import { parseMemo } from "../src/memo";

describe("parseMemo", () => {
  it("rejects an empty memo with its message", () => {
    expect(() => parseMemo("")).to.throw("memo required");
  });
});
