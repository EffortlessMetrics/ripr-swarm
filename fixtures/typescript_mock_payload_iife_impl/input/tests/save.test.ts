import { save } from "../src/save";

function recordTwo(value: number): void {
    void value;
}

test("save", () => {
    const db = { write: jest.fn((() => recordTwo)()) };
    save(db, 1);
    expect(db.write).toHaveBeenCalledWith(2);
});
