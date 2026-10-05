import { expect, it } from "vitest";
import { buildOrderLine } from "../src/order";

it("builds the full line", () => {
  expect(buildOrderLine("A-1", 3, 4)).toStrictEqual({ sku: "A-1", qty: 3, total: 12 });
});
