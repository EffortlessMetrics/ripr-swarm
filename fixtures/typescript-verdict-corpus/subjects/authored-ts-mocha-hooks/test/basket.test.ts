import { expect } from "chai";
import { Basket } from "../src/basket";

describe("Basket", () => {
  let basket: Basket;

  beforeEach(() => {
    basket = new Basket();
  });

  context("when empty", () => {
    it("totals zero", () => {
      expect(basket.total()).to.equal(0);
    });
  });

  context("with two items", () => {
    beforeEach(() => {
      basket.add(300);
      basket.add(200);
    });

    it("totals the prices", () => {
      expect(basket.total()).to.equal(500);
    });

    it("has some items", () => {
      expect(basket.count()).to.be.greaterThan(0);
    });
  });
});
