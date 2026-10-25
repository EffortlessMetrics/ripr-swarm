export function notify(id: number, sender: { send: (value: number) => void }): void {
  sender.send(2 * id);
}
