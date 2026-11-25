from routeclients.mini import MiniApp

app = MiniApp()


@app.get("/stock")
def stock():
    return {"left": 5}
