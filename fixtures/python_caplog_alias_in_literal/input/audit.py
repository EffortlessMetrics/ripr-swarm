import logging

log = logging.getLogger("audit")


def check_balance(balance):
    if balance < 10:
        log.warning("msgs: 5")
        return False
    return True
