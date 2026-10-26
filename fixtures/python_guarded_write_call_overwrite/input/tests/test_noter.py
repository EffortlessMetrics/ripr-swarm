from shapes.noter import Recorder

def test_note():
    rec = Recorder()
    rec.last = rec.note("error")
    assert rec.last == None
