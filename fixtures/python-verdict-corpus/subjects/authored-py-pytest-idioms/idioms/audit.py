import logging

log = logging.getLogger("idioms.audit")


def check_balance(balance):
    if balance < 10:
        log.warning("low balance: %s", balance)
        return False
    return True
