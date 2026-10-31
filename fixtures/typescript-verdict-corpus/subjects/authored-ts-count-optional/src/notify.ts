export function notify(id: number, sender: { send: (value: number) => void }): void {
  sender.send(id * 2);
}
