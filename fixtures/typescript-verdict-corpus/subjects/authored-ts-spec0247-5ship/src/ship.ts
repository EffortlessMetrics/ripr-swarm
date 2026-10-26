export function ship(notifier: { send(m: string): void }, cb: (m: string) => void): void {
  notifier.send("shipped");
  cb("done");
}
