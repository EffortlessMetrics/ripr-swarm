import { orderSumMultiline } from "../src/orderSumMultiline";

test("orderSumMultiline", () => {
  return orderSumMultiline([10, 20])
    .then((sum) => { expect(sum).toBe(30); }, () => {});
});
