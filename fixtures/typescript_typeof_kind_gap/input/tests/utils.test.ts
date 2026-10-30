import { resolveValue } from "../src/utils";

describe("resolveValue", () => {
    test("returns a number unchanged", () => {
        expect(resolveValue(42)).toBe(42);
    });

    test("returns the string 'function' unchanged", () => {
        expect(resolveValue("function" as any)).toBe("function");
    });
});
