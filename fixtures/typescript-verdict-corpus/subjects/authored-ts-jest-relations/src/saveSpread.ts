export interface SpreadDb {
  write(v: number): void;
  flush(): void;
}

export function saveSpread(db: SpreadDb, v: number): void {
  db.write(v + 1);
}
