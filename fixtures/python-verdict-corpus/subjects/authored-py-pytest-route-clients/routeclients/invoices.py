from routeclients.mini import MiniApp

app = MiniApp()


@app.get("/invoices")
def invoices():
    return {"total": 30}
