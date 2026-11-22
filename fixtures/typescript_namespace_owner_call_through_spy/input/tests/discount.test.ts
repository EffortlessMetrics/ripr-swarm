// Control for typescript_adversarial_stubbed_namespace_owner: the spy has no
// fabrication, so the call runs the changed owner and the exact-value
// assertion observes its result.
import * as pricing from "../src/discount";

let spy: jest.SpyInstance;

beforeEach(() => {
    spy = jest.spyOn(pricing, "applyDiscount");
});

test("applyDiscount at threshold discounts", () => {
    expect(pricing.applyDiscount(100, 100)).toBe(90);
    expect(spy).toHaveBeenCalledTimes(1);
});
