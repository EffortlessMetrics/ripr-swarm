import { expect, test } from "@playwright/test";
import { dims } from "../src/soft";

test("reports both dimensions", () => {
  const d = dims(3, 4);
  expect.soft(d.area).toBe(12);
  expect.soft(d.perimeter).toBe(14);
});
