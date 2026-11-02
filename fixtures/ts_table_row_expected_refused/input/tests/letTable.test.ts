import { describe, expect, test } from "vitest";
import { letTable } from "../src/letTable";

describe("letTable", () => {
    let cases = [
        { input: 999, out: "silver" },
        { input: 1000, out: "gold" },
    ];

    for (const t of cases) {
        test("tier by points", () => {
            expect(letTable(t.input)).toBe(t.out);
        });
    }
});
