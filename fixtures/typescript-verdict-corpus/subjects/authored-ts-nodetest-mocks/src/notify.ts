export interface Mailer {
  send(to: string, body: string): void;
}

export function welcome(mailer: Mailer, name: string): void {
  mailer.send(`${name}@example.test`, `Welcome, ${name}`);
}
