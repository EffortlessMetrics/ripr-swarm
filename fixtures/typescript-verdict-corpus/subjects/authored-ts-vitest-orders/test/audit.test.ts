import { expect, it, vi } from "vitest";
import { recordShipment } from "../src/audit";

it("logs the shipment", () => {
  const log = vi.fn();
  recordShipment(log, 42);
  expect(log).toHaveBeenCalledWith("shipped order 42");
});
