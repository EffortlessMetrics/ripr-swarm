import json, types, sys
from pathlib import Path
p=Path(sys.argv[1]); data=json.loads(p.read_text()); results=[]
assert len(data['cases']) == 11
assert len({case['id'] for case in data['cases']}) == 11
for case in data['cases']:
    for version in ['old','new']:
        src=types.ModuleType('src'); src.__path__=[]
        shop=types.ModuleType('src.shop')
        exec(data['source_'+version], shop.__dict__)
        src.shop=shop
        sys.modules['src']=src; sys.modules['src.shop']=shop
        namespace={}
        exec(case['test'],namespace)
        passed=True; failure=None
        try:
            if 'test_fee' in namespace:
                namespace['test_fee']()
            else:
                namespace['FeeTest']('test_fee').test_fee()
        except Exception as err:
            passed=False; failure=type(err).__name__
        assert passed == case[version+'_test_passes'], (case['id'], version, passed, failure)
        expected_failure = None if case[version+'_test_passes'] else 'AssertionError'
        assert failure == expected_failure, (case['id'], version, failure, expected_failure)
        results.append(dict(case=case['id'],version=version,passed=passed,failure=failure))
Path(sys.argv[2]).write_text(json.dumps(results,indent=2)+'\n')
print('22 native Python executions matched independently literal expectations')
