export function save(db: { write: (value: number) => void }, v: number): void {
    db.write(v + 2);
}
