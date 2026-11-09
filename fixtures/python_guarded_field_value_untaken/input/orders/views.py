def order_view(request, total):
    if total < 0:
        return {"status": 422}
    return {"status": 200}
