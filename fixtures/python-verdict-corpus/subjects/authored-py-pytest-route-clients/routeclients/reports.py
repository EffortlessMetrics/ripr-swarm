from routeclients.mini import MiniApp

app = MiniApp()


@app.get("/reports")
def reports():
    return {"rows": 4}


def build_app():
    return app
