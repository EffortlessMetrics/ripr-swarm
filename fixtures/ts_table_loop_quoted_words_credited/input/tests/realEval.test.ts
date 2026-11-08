import { describe, expect, test } from "vitest";
import { realEval } from "../src/realEval";

describe("realEval", () => {
    test("for...of table", () => {
        const rows: Array<[number, string]> = [[1000, "gold"]];
        for (const [points, tier] of rows) {
            expect(realEval(points)).toBe(tier);
            eval("tier = 'x'");
        }
    });
});
