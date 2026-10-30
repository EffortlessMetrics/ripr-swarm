export const mailer = {
  send(message: string): string {
    return message;
  },
};

export function announceMail(id: number): void {
  mailer.send("order " + id);
}
