import { expect, it } from "vitest";
import { Counter } from "../src/counter";

it("counts up from one", () => {
  const counter = new Counter();
  expect(counter.next()).toBe(1);
  expect(counter.next()).toBe(2);
});
