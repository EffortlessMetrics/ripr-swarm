import { describe, expect, test } from "vitest";
import { eachPattern } from "../src/eachPattern";

describe("eachPattern", () => {
    test.each([
        { points: 999, want: "silver" },
        { points: 1000, want: "gold" },
    ])("tier by points", ({ points, want }) => {
        expect(eachPattern(points)).toBe(want);
    });
});
