import { describe, expect, test } from "vitest";
import { realThis } from "../src/realThis";

describe("realThis", () => {
    test.each([
        { input: 999, out: "silver" },
        { input: 1000, out: "gold" },
    ])("tier by points", function (t) {
        expect(realThis(t.input)).toBe(t.out + (this === undefined ? "" : ""));
    });
});
