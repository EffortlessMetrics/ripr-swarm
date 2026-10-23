import sys

if sys.version_info >= (3, 0):

    def versioned():
        return "new"

else:

    def versioned():
        return "old"
