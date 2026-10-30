export function deadBranch(a: number, b: number): number {
  return a * b;
}

export function liveBranch(a: number, b: number): number {
  return a * b;
}

export function skippedTest(a: number, b: number): number {
  return a * b;
}

export function skipOption(a: number, b: number): number {
  return a * b;
}

export function todoTest(a: number, b: number): number {
  return a * b;
}

export function platformGuarded(a: number, b: number): number {
  return a * b;
}

export function platformMatched(a: number, b: number): number {
  return a * b;
}

export function unregistered(a: number, b: number): number {
  return a * b;
}

export function registered(a: number, b: number): number {
  return a * b;
}

export function inputAsserted(a: number, b: number): number {
  return a * b;
}

export function resultAsserted(a: number, b: number): number {
  return a * b;
}

export function swallowedAssert(a: number, b: number): number {
  return a * b;
}

export function rethrownAssert(a: number, b: number): number {
  return a * b;
}

export function tautology(a: number, b: number): number {
  return a * b;
}

export function truthyOnly(a: number, b: number): number {
  return a * b;
}

export function emptyTable(a: number, b: number): number {
  return a * b;
}

export function filledTable(a: number, b: number): number {
  return a * b;
}

export function wrappedSwallow(a: number, b: number): number {
  return a * b;
}

export function wrappedThrough(a: number, b: number): number {
  return a * b;
}

export async function floatingPromise(a: number, b: number): Promise<number> {
  return a * b;
}

export async function awaitedPromise(a: number, b: number): Promise<number> {
  return a * b;
}

export function mockedSend(orderId: number, sender: { send: (value: number) => void }): void {
  sender.send(orderId * 2);
}

export function mockedSendArgs(orderId: number, sender: { send: (value: number) => void }): void {
  sender.send(orderId * 2);
}

export function siblingArm(kind: string, qty: number): number {
  if (kind === "single") {
    return qty * 2;
  }
  return qty * 3;
}

export function editedArm(kind: string, qty: number): number {
  if (kind === "single") {
    return qty * 2;
  }
  return qty * 3;
}
