import { describe, expect, test } from "vitest";
import { quotedTitle } from "../src/quotedTitle";

describe("quotedTitle", () => {
    test.each([
        { input: 999, out: "silver" },
        { input: 1000, out: "gold" },
    ])("tier works with this exact boundary", (t) => {
        expect(quotedTitle(t.input)).toBe(t.out);
    });
});
