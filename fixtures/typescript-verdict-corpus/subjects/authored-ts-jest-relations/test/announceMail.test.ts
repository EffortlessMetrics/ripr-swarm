import { announceMail, mailer } from "../src/announceMail";

test("announceMail", () => {
  const send = jest.spyOn(mailer, "send");
  announceMail(7);
  expect(send).toHaveBeenCalledWith("order 7");
});
