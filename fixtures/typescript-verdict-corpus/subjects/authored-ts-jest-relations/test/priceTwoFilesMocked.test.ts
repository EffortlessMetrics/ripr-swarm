import { priceTwoFiles } from "../src/priceTwoFiles";

jest.mock("../src/fee33", () => ({ fee33: () => 5 }));

test("priceTwoFiles mocked", () => {
  expect(priceTwoFiles(150)).toBe(5);
});
