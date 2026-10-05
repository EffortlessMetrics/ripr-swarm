import { priceOtherMocked } from "../src/priceOtherMocked";

jest.mock("../src/other24");

test("priceOtherMocked", () => {
  expect(priceOtherMocked(150)).toBe(140);
});
