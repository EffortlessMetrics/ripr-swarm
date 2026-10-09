export interface Db {
  write(v: number): void;
}

export function save(db: Db, v: number): void {
  db.write(v + 1);
}
