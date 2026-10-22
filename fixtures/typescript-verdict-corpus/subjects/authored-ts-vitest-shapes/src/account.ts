export class Account {
  private balance: number;

  constructor(opening: number) {
    this.balance = opening;
  }

  deposit(amount: number): number {
    this.balance += amount;
    return this.balance;
  }

  get overdrawn(): boolean {
    return this.balance < 0;
  }
}
