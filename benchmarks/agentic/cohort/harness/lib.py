import os, subprocess, shutil, json, sys
from tasks import TASKS
import json as _j, os as _o
for _f in ('tasks_big', 'tasks_real'):
    try:
        BIG = __import__(_f).__dict__.get('BIG') or __import__(_f).__dict__.get('REAL')
        for _k,_t in BIG.items():
            _m=_o.path.join(_o.path.dirname(_o.path.abspath(__file__)),'..','mutants',_k+'.json')
            if _o.path.exists(_m): _t['mutants']=[tuple(x) for x in _j.load(open(_m))]
            TASKS[_k]=_t
    except ImportError: pass


# The catalog keeps its historical imports; workspace/scoring own execution.
from workspace import GIT, build_repo, cargo_test, score, sh, write
