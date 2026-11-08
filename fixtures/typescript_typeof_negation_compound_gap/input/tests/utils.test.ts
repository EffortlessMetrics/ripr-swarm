import { pick } from "../src/utils";

describe("pick", () => {
    test("returns a number unchanged when the flag is off", () => {
        expect(pick(42, false)).toBe(42);
    });
});
