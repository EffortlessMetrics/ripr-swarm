from routeclients.mini import MiniApp

app = MiniApp()


@app.get("/orders")
def archived_orders():
    return {"count": 5}
