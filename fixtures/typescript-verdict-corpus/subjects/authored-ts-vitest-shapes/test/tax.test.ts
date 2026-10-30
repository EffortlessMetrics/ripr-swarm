import { expect, it } from "vitest";
import { tax } from "../src/tax";

it("taxes a US purchase", () => {
  expect(tax("US", 100)).toBeCloseTo(7);
});
