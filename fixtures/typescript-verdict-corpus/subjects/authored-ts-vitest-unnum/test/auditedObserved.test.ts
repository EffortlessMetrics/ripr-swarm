import { expect, test, vi } from "vitest";
import { chargeObserved } from "../src/auditedObserved";

test("charge returns the net and writes the audit entry", () => {
  const sideEffectLog: unknown[][] = [];
  const spy = vi.spyOn(console, "log").mockImplementation((...args: unknown[]) => {
    sideEffectLog.push(args);
  });
  expect(chargeObserved(100, 10)).toBe(90);
  spy.mockRestore();
  expect(sideEffectLog).toEqual([["observed", 900]]);
});
