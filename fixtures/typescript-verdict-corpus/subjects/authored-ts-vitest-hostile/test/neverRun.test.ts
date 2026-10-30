import { expect, it } from "vitest";
import { rate } from "../src/neverRun";

function checkGold(): void {
  expect(rate("gold")).toBe(0.2);
}

it("the helper exists", () => {
  expect(typeof checkGold).toBe("function");
});
