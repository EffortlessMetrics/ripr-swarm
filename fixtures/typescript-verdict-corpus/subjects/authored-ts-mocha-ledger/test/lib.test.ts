import { expect } from "chai";
import { price } from "../src/lib";

describe("price", () => {
  it("price", () => {
    expect(price(150)).to.equal(140);
  });
});
