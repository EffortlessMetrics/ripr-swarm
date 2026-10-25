from shapes.noter import Recorder

def test_note():
    rec = Recorder()
    rec.note("error")
    assert rec.last == "settled"
