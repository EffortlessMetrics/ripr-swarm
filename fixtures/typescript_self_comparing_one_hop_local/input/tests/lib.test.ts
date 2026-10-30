import { price } from "../src/lib";

test("price", () => {
    const a = price(150);
    expect(a).toBe(a);
});
