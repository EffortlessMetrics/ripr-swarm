import { priceFeeMocked } from "../src/priceFeeMocked";

jest.mock("../src/fee31", () => ({ fee31: () => 5 }));

test("priceFeeMocked", () => {
  expect(priceFeeMocked(150)).toBe(5);
});
