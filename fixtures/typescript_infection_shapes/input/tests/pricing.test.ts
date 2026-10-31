import { describe, it, expect } from "vitest";
import { bulk, tier, label, total, div, cap, disc, say, sum, same } from "../src/pricing";
describe("pricing", () => {
  it("bulk", () => { expect(bulk(20)).toBeGreaterThan(0); });
  it("tier", () => { expect(tier(80)).toBeTruthy(); });
  it("label", () => { expect(label(3)).toBeTruthy(); });
  it("total", () => { expect(total([1, 2])).toBe(3); });
  it("div", () => { expect(div(4, 2)).toBe(2); });
  it("cap", () => { expect(cap({ max: 3 })).toBe(3); });
  it("disc", () => { expect(disc(100)).toBeGreaterThan(0); });
  it("say", () => { expect(say("a", true)).toBe("A"); });
  it("sum", () => { expect(sum([1, 2])).toBe(3); });
  it("same", () => { expect(same(3)).toBe(3); });
});
