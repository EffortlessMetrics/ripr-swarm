import { expect } from "chai";
import { clamp, describeRange, mean } from "../src/clamp";

describe("clamp", () => {
  it("clamps above", () => {
    expect(clamp(15, 0, 10)).to.equal(10);
  });

  it("keeps a value in range", () => {
    expect(clamp(5, 0, 10)).to.equal(5);
  });
});

describe("mean", () => {
  it("is close to the true mean", () => {
    expect(mean([1, 2, 4])).to.be.closeTo(2.333, 0.01);
  });
});

describe("describeRange", () => {
  it("formats a range", () => {
    expect(describeRange(1, 4)).to.include("..");
  });

  it("refuses an inverted range", () => {
    expect(() => describeRange(4, 1)).to.throw(RangeError, "lo above hi");
  });
});
