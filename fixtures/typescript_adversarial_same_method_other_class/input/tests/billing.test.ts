import { PaymentProcessor } from '../src/billing';

test('validates a trimmed card number', () => {
  const proc = new PaymentProcessor();
  expect(proc.validate("card1234")).toBe(true);
});
