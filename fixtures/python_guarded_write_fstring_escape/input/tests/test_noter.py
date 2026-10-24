from shapes.noter import Recorder

def test_note():
    rec = Recorder()
    try:
        rec.note("error")
    except (ValueError, AssertionError, SystemExit):
        pass
    assert rec.last == None
