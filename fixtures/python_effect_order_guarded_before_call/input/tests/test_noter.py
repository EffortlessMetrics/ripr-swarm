from shapes.noter import Recorder

def test_note():
    rec = Recorder()
    assert rec.last == None; rec.note("error")
