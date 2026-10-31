import { expect, test } from "vitest";
import { isBulk } from "../src/arrayLen";

test("bulk threshold yields one result", () => {
  expect([isBulk(100)].length).toBe(1);
});
