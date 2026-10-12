import { expect } from "chai";
import { reserve } from "../src/reserve";

describe("reserve", () => {
  it("changes the balance", () => {
    expect(reserve(500)).to.not.equal(500);
  });
});
