import { expect } from "chai";
import { isOpen } from "../src/hours";

describe("isOpen", () => {
  it("is open at noon", () => {
    expect(isOpen(12)).to.be.true;
  });
});
