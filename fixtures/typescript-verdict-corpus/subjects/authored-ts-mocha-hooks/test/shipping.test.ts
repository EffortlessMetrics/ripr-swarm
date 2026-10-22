import { expect } from "chai";
import { shippingCents } from "../src/shipping";

describe("shippingCents", () => {
  describe("a light parcel", () => {
    it("pays the small rate at the limit", () => {
      expect(shippingCents(500)).to.equal(399);
    });
  });

  describe("a heavy parcel", () => {
    it("pays the large rate", () => {
      expect(shippingCents(2000)).to.equal(899);
    });
  });
});
