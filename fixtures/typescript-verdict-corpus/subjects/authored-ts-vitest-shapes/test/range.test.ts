import { expect, it } from "vitest";
import { range } from "../src/range";

it("yields zero up to but excluding n", () => {
  expect([...range(3)]).toEqual([0, 1, 2]);
});
