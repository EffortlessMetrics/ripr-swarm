import { expect } from "chai";
import { load } from "../src/loader";

describe("load", () => {
  it("calls back with three", (done) => {
    load((value) => {
      expect(value).to.equal(3);
      done();
    });
  });
});
