import { describe, expect, test } from "vitest";
import { trailingNote } from "../src/trailingNote";

describe("trailingNote", () => {
    test.each([
        { input: 999, out: "silver" },
        { input: 1000, out: "gold" },
    ])("tier by points", (t) => {
        expect(trailingNote(t.input)).toBe(t.out); // boundary row
    });
});
