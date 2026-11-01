import { describe, expect, test } from "vitest";
import { eachTable } from "../src/eachTable";

describe("eachTable", () => {
    test.each([
        { input: 999, out: "silver" },
        { input: 1000, out: "gold" },
    ])("tier by points", (t) => {
        expect(eachTable(t.input)).toBe(t.out);
    });
});
