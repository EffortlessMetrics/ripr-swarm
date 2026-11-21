export function notify(id, sender) {
  sender.send(2 * id);
}
