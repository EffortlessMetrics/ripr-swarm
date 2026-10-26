import logging

log = logging.getLogger("audit")


def check_balance(balance):
    if balance < 10:
        log.warning("balance low: %s", balance)
        return False
    return True
