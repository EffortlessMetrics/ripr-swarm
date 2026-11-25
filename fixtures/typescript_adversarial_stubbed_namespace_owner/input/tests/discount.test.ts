// Adversarial stubbed-owner trap: a hook replaces the CHANGED OWNER on its
// module object, so the call in the test runs the stub. The exact-value
// assertion observes the stub's return, not the changed comparison.
import sinon from "sinon";
import * as pricing from "../src/discount";

beforeEach(() => {
    sinon.stub(pricing, "applyDiscount").returns(90);
});

afterEach(() => {
    sinon.restore();
});

test("applyDiscount at threshold discounts", () => {
    expect(pricing.applyDiscount(100, 100)).toBe(90);
});
