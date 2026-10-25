import { expect, test } from "vitest";
import { chargeAudited } from "../src/auditedLog";

test("charge applies the percent to 100", () => {
  expect(chargeAudited(100, 10)).toBe(90);
});

test("charge applies the percent to 50", () => {
  expect(chargeAudited(50, 10)).toBe(45);
});
