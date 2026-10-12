import { orderTotal } from "../src/order";

test("order total adds every item", () => {
  return orderTotal([10, 20]).then((total) => {
    expect(total).toBe(30);
  });
});
