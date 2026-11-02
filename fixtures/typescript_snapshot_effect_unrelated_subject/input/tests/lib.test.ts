import { notify } from "../src/lib";

test("notify", () => {
    notify(7);
    const unrelated = "x";
    expect(unrelated).toMatchInlineSnapshot(`"x"`);
});
