import { describe, expect, test } from "vitest";
import { blockNote } from "../src/blockNote";

describe("blockNote", () => {
    test.each([
        { input: 999, out: "silver" },
        { input: 1000, out: "gold" },
    ])("tier by points", (t) => {
        /* the boundary is inclusive */
        expect(blockNote(t.input)).toBe(t.out);
    });
});
