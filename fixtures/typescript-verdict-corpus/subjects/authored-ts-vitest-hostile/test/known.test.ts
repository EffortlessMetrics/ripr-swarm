import { expect, it } from "vitest";
import { answer } from "../src/known";

it.fails("known wrong answer", () => {
  expect(answer()).toBe(41);
});
