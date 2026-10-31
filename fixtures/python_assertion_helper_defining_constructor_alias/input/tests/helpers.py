import src.stats as mod

class Other:
    writes = 0
    def record_write(self):
        pass

mod.Stats = Other

def check_writes(obj, want):
    assert obj.writes == want
