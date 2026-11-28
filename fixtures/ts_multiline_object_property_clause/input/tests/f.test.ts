import { f } from "../src/f";

describe("f", () => {
    test("takes one input", () => {
        expect(f(42, false)).toEqual({ callable: false });
    });
});
