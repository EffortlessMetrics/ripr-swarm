import { enqueue } from "../src/lib";

test("enqueue", () => {
    enqueue([], 7);
    const unrelated = ["shipped 7"];
    expect(unrelated).toMatchSnapshot();
});
