import { expect } from "chai";
import { loadLimit } from "../src/limit";

describe("loadLimit", () => {
  it("loads the tier limit", async () => {
    expect(await loadLimit("gold")).to.equal(5000);
    expect(await loadLimit("silver")).to.equal(1000);
  });
});
