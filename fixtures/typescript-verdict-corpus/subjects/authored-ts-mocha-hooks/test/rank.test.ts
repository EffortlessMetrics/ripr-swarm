import { expect } from "chai";
import { medal, rank } from "../src/rank";

describe("rank", () => {
  it("awards gold at ninety", () => {
    expect(rank(90)).to.equal("gold");
  });

  it.skip("awards silver below ninety", () => {
    expect(rank(89)).to.equal("silver");
  });
});

describe.skip("medal", () => {
  it("awards bronze at fifty", () => {
    expect(medal(50)).to.equal("bronze");
  });
});
