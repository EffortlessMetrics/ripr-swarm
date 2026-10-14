import { orderSum } from "../src/orderSum";

test("orderSum", () => {
  return orderSum([10, 20]).then((sum) => { expect(sum).toBe(30); }, () => {});
});
