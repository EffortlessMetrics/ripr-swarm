import pathlib,json
p=pathlib.Path(__file__).parent
j=json.loads((p/'cases/owner-crate-glob/check.json').read_text())
assert j['analysis_outcome']['analysis_complete'] is True
headers=[x for x in j['findings'] if x['probe']['family']=='match_arm' and x['probe']['expression']=='match lease.elapsed']
assert len(headers)==1,'witness setup must retain exact whole-match header'
assert headers[0]['classification']=='exposed',f"Future closed-primitive-identity positive is not admitted: {headers[0]['id']} = {headers[0]['classification']}; current conservative refusal is retained pending new authority"
