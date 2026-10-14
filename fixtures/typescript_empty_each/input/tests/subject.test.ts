import { test, it, describe, suite, expect } from "vitest";
import { isAdult } from "../src/subject";

test.each([])("empty test", () => { expect(isAdult(18)).toBe(true); });
it.only.each(([] as const))("empty focused test", () => { expect(isAdult(18)).toBe(true); });
describe.each([])("empty suite", () => {
  test("unregistered child", () => { expect(isAdult(18)).toBe(true); });
});
suite.each(([] satisfies readonly unknown[]))("empty alias suite", () => {
  test("unregistered alias child", () => { expect(isAdult(18)).toBe(true); });
});
