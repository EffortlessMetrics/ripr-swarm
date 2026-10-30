def send(client, user, message):
    if not user.get("email"):
        return False
    client.deliver(user["email"], message.upper())
    return True
