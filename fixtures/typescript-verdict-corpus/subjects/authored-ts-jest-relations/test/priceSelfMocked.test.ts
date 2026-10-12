import { priceSelfMocked } from "../src/priceSelfMocked";

jest.mock("../src/priceSelfMocked");

test("priceSelfMocked", () => {
  expect(priceSelfMocked(150)).toBe(undefined);
});
