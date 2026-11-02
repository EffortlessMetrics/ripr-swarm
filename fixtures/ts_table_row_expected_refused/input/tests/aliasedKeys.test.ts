import { describe, expect, test } from "vitest";
import { aliasedKeys } from "../src/aliasedKeys";

describe("aliasedKeys", () => {
    test.each([
        { input: { points: 1000 }, out: "silver" },
        { input: { points: 999 }, out: "silver" },
    ])("tier by points", ({ input: a, input: b, out }) => {
        Object.assign(a, { points: 5 });
        expect(aliasedKeys(b)).toBe(out);
    });
});
