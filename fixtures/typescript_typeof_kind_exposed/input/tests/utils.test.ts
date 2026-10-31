import { resolveValue } from "../src/utils";

describe("resolveValue", () => {
    test("returns a number unchanged", () => {
        expect(resolveValue(42)).toBe(42);
    });

    test("calls a function input", () => {
        expect(resolveValue(() => 42)).toBe(42);
    });
});
