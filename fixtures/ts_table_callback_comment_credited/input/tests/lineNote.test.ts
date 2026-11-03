import { describe, expect, test } from "vitest";
import { lineNote } from "../src/lineNote";

describe("lineNote", () => {
    test.each([
        { input: 999, out: "silver" },
        { input: 1000, out: "gold" },
    ])("tier by points", (t) => {
        // the boundary is inclusive
        expect(lineNote(t.input)).toBe(t.out);
    });
});
