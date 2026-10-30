import logging

log = logging.getLogger("absence.audit")


def settle(amount):
    if amount > 0:
        log.warning("settled: %s", amount)
        return True
    return False


def notify(user):
    if user != "":
        log.warning("notified: %s", user)
        return True
    return False


def refund(order):
    if order > 0:
        log.warning("refunded: %s", order)
        return True
    return False


def ship(parcel):
    if parcel > 0:
        log.warning("shipped: %s", parcel)
        return True
    return False


def archive(record):
    if record > 0:
        log.warning("archived: %s", record)
        return True
    return False


def purge(record):
    if record > 0:
        log.warning("purged: %s", record)
        return True
    return False


def rotate(key):
    if key > 0:
        log.warning("rotated: %s", key)
        return True
    return False
