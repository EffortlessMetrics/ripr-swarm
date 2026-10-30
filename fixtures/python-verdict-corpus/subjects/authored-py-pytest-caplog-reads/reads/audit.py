import logging

log = logging.getLogger("reads.audit")


def low_balance(balance):
    if balance < 10:
        log.warning("low balance: %s", balance)
        return False
    return True


def low_stock(count):
    if count < 3:
        log.warning("low stock: %s", count)
        return False
    return True


def overdrawn(balance):
    if balance < 0:
        log.warning("overdrawn: %s", balance)
        return False
    return True


def frozen(account):
    if account == "frozen":
        log.warning("account frozen: %s", account)
        return False
    return True


def locked(account):
    if account == "locked":
        log.warning("account locked: %s", account)
        return False
    return True


def slow_query(millis):
    if millis > 500:
        log.warning("slow query: %s", millis)
        return False
    return True


def expiring(days):
    if days < 7:
        log.warning("card expiring: %s", days)
        return False
    return True
