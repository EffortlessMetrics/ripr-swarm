from routeclients.mini import MiniApp

app = MiniApp()


@app.get("/orders")
def orders():
    return {"count": 2}
