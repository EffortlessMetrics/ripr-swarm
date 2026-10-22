import os


def home_dir():
    return os.environ.get("HOME_DIR", "/tmp")
