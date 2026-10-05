import { expect } from "chai";
import { parse } from "../src/parse";

describe("parse", () => {
  it("parse", () => {
    expect(() => parse("")).to.throw("empty");
  });
});
