import { describe, expect, test } from "vitest";
import { derivedTable } from "../src/derivedTable";

describe("derivedTable", () => {
    const cases = [
        { input: 999, out: "silver" },
        { input: 1000, out: "gold" },
    ];

    for (const t of cases.map((row) => row)) {
        test("tier by points", () => {
            expect(derivedTable(t.input)).toBe(t.out);
        });
    }
});
