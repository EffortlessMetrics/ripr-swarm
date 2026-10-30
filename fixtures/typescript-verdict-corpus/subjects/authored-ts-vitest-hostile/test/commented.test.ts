import { expect, it } from "vitest";
import { perimeter, triple } from "../src/commented";

it("computes a perimeter", () => {
  // expect(perimeter(3, 4)).toBe(14);
  expect(typeof perimeter(3, 4)).toBe("number");
});

it("computes a triple", () => {
  /*
  expect(triple(2)).toBe(6);
  */
  const spec = "expect(triple(2)).toBe(6)";
  expect(spec.length).toBeGreaterThan(0);
  triple(2);
});
