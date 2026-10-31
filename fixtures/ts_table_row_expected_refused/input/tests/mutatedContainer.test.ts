import { describe, expect, test } from "vitest";
import { mutatedContainer } from "../src/mutatedContainer";

describe("mutatedContainer", () => {
    const cases = [
        { input: { points: 999 }, out: "silver" },
        { input: { points: 1000 }, out: "silver" },
    ];

    for (const t of cases) {
        test("tier by points", () => {
            Object.assign(t.input, { points: 5 });
            expect(mutatedContainer(t.input)).toBe(t.out);
        });
    }
});
