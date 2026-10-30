export class Shape {
  area(): number {
    return 0;
  }

  label(): string {
    return `shape of area ${this.area()}`;
  }
}

export class Square extends Shape {
  constructor(private side: number) {
    super();
  }

  area(): number {
    return this.side * this.side;
  }
}
