import { priceTwoFiles } from "../src/priceTwoFiles";

test("priceTwoFiles unmocked", () => {
  expect(priceTwoFiles(150)).toBe(140);
});
