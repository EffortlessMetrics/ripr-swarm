import { resolveValue } from "../src/utils";

describe("resolveValue", () => {
    test("returns a non-function value unchanged", () => {
        expect(resolveValue(42)).toBe(42);
    });
});
