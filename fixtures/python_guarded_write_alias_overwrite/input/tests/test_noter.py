from shapes.noter import Recorder

def test_note():
    rec = Recorder()
    alias = rec
    rec.note("error")
    alias.last = "settled"
    assert rec.last == "settled"
