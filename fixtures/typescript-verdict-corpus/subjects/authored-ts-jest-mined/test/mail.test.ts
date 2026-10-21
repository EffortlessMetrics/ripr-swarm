import { announceMail, mailer } from "../src/mail";

test("announceMail", () => {
  const send = jest.spyOn(mailer, "send");
  announceMail(7);
  expect(send).toHaveBeenCalledWith("order 7");
});
