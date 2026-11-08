import { describe, expect, test } from "vitest";
import { identifierCell } from "../src/identifierCell";

describe("identifierCell", () => {
    const gold = "gold";
    const cases = [
        { input: 999, out: "silver" },
        { input: 1000, out: gold },
    ];

    for (const t of cases) {
        test("tier by points", () => {
            expect(identifierCell(t.input)).toBe(t.out);
        });
    }
});
