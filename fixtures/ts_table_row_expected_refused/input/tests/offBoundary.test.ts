import { describe, expect, test } from "vitest";
import { offBoundary } from "../src/offBoundary";

describe("offBoundary", () => {
    const cases = [
        { input: 100, out: "silver" },
        { input: 5000, out: "gold" },
    ];

    for (const t of cases) {
        test("tier by points", () => {
            expect(offBoundary(t.input)).toBe(t.out);
        });
    }
});
