import { describe, expect, test } from "vitest";
import { quotedWithTitle } from "../src/quotedWithTitle";

describe("quotedWithTitle", () => {
    test("for...of table", () => {
        const rows: Array<[number, string]> = [
            [999, "silver"],
            [1000, "gold"],
        ];
        for (const [points, tier] of rows) {
            expect(quotedWithTitle(points)).toBe(tier); // works with spaces
        }
    });
});
