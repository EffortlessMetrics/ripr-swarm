import { priceUnrelatedMocked } from "../src/priceUnrelatedMocked";

jest.mock("../src/unrelated32");

test("priceUnrelatedMocked", () => {
  expect(priceUnrelatedMocked(150)).toBe(140);
});
