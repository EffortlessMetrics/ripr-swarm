import { fmt } from "../src/fmt";

describe("fmt", () => {
    test("one input", () => {
        expect(fmt(42)).toBe("42");
    });
});
