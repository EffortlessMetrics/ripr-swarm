import sys
sys.version_info, extra = (2, 7), 0

if sys.version_info >= (3, 0):
    def versioned():
        return 'newer'
else:
    def versioned():
        return 'new'
