import { describe, expect, test } from "vitest";
import { missingKey } from "../src/missingKey";

describe("missingKey", () => {
    const cases = [
        { input: 999, out: "silver" },
        { input: 1000 },
    ];

    for (const t of cases) {
        test("tier by points", () => {
            expect(missingKey(t.input)).toBe(t.out);
        });
    }
});
