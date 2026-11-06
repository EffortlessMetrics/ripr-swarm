import { describe, expect, test } from "vitest";
import { loopTable } from "../src/loopTable";

describe("loopTable", () => {
    const cases = [
        { input: 999, out: "silver" },
        { input: 1000, out: "gold" },
    ];

    for (const t of cases) {
        test("tier by points", () => {
            expect(loopTable(t.input)).toBe(t.out);
        });
    }
});
