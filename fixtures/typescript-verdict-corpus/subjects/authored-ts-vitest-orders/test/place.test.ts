import { expect, it, vi } from "vitest";
import { placeOrder } from "../src/place";

vi.mock("../src/stock", () => ({ reserveStock: () => 5 }));

it("places the order", () => {
  expect(placeOrder(10, 3)).toBe("placed");
});
