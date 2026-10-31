from src.stats import Stats

def ignore(self):
    pass

Stats.record_write = ignore

def check_writes(obj, want):
    assert obj.writes == want
