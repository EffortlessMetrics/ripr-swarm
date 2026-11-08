import { fmt } from "../src/fmt";

describe("fmt", () => {
    test("one input", () => {
        expect(fmt("a")).toBe("a");
    });
});
