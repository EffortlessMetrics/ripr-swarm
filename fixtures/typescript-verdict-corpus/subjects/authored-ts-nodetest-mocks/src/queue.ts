export class Queue<T> {
  private items: T[] = [];

  push(item: T): number {
    this.items.push(item);
    return this.items.length;
  }

  shift(): T | undefined {
    return this.items.shift();
  }
}
