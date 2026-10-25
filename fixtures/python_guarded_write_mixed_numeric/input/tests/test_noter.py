from shapes.noter import Recorder

def test_note():
    rec = Recorder()
    rec.note(9007199254740993)
    assert rec.last == None
