import { describe, expect, test } from "vitest";
import { exportedTable } from "../src/exportedTable";

export const cases = [
    { input: 999, out: "silver" },
    { input: 1000, out: "gold" },
];

describe("exportedTable", () => {
    for (const t of cases) {
        test("tier by points", () => {
            expect(exportedTable(t.input)).toBe(t.out);
        });
    }
});
