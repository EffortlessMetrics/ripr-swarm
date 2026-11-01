import { describe, expect, test } from "vitest";
import { pushedTable } from "../src/pushedTable";

describe("pushedTable", () => {
    const cases = [
        { input: 999, out: "silver" },
        { input: 1000, out: "gold" },
    ];
    cases.push({ input: 1000, out: "silver" });

    for (const t of cases) {
        test("tier by points", () => {
            expect(pushedTable(t.input)).toBe(t.out);
        });
    }
});
