import * as sums from "../src/orderSumMember";

test("orderSumMember", () => {
  return sums.orderSumMember([10, 20]).then((sum) => { expect(sum).toBe(30); }, () => {});
});
