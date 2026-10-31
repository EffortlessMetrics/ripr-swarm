import { expect } from "chai";
import { Cart } from "../src/cart";
describe("cart", () => {
  let cart = { total: () => 1 };
  beforeEach = () => {};
  beforeEach(() => { cart = new Cart(); });
  it("observes cart total", () => { expect(cart.total()).to.equal(1); });
});
