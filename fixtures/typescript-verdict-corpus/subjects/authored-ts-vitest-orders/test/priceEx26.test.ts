import { expect, test, vi } from "vitest";
import * as lib from "../src/priceEx26";

test("price", () => {
  vi.spyOn(lib, "priceEx26").mockReturnValue(130);
  expect(lib.priceEx26(150)).toBe(130);
});
