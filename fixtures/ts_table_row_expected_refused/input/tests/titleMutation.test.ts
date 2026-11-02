import { describe, expect, test } from "vitest";
import { titleMutation } from "../src/titleMutation";

describe("titleMutation", () => {
    const cases = [
        { input: { points: 999 }, out: "silver" },
        { input: { points: 1000 }, out: "silver" },
    ];

    for (const t of cases) {
        test(String(Object.assign(t.input, { points: 5 })), () => {
            expect(titleMutation(t.input)).toBe(t.out);
        });
    }
});
