from shapes.ctx import Recorder


def test_recorder_notes_enter_and_exit():
    with Recorder() as rec:
        pass
    assert rec.events == ["enter", "exit"]
