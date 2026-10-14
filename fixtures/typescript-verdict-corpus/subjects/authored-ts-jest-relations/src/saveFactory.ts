export interface FactoryDb {
  write(v: number): void;
}

export function saveFactory(db: FactoryDb, v: number): void {
  db.write(v + 1);
}
