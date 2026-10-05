import { expect } from "chai";
import { bonusPoints } from "../src/bonus";

describe("bonusPoints", () => {
  it("pays no bonus at exactly 500", () => {
    expect(bonusPoints(500)).to.equal(0);
  });

  it("pays the bonus just above 500", () => {
    expect(bonusPoints(501)).to.equal(25);
  });
});
