import { price } from "@/lib";

test("price", () => {
  expect(price(150)).toBe(140);
});
