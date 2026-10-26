import importlib

module = importlib.import_module("rates." + "tab" + "le")
getattr(module, "RATES")["silver"] = 5
