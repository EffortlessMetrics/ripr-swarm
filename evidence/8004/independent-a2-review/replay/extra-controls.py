import pathlib,json
root=pathlib.Path('/workspace/review8004')
out=pathlib.Path('/workspace/review8004-evidence/extras');out.mkdir(exist_ok=True)
base=(root/'fixtures/inherent_receiver_async_operand/input/src/lib.rs').read_text().split('#[cfg(test)]')[0]
cases=[]
stimuli=[
 ('direct-debug-async', '', 'debug_assert_eq!({ let _ = async move { JsonValue::Object(7).as_object() }; 7 }, 7);',None,False),
 ('direct-ne-async','', 'assert_ne!({ let _ = async { JsonValue::Object(7).as_object() }; 7 }, 8);',None,False),
 ('direct-nested-async','', 'assert!({ let _ = async { let _ = async move { JsonValue::Object(7).as_object() }; }; true });',None,False),
 ('direct-debug-mixed','', 'debug_assert_eq!({ let _ = async { JsonValue::Object(7).as_object() }; JsonValue::Object(7).as_object() }, Some(7));','direct_owner_call',True),
 ('helper-last-hop-async','fn read_as_object() -> Option<u32> { assert_eq!({ let _ = async { JsonValue::Object(7).as_object() }; 7 }, 7); Some(7) }','assert_eq!(read_as_object(), Some(7));',None,False),
 ('helper-last-hop-mixed','fn read_as_object() -> Option<u32> { assert_eq!({ let _ = async { JsonValue::Object(7).as_object() }; JsonValue::Object(7).as_object() }, Some(7)); Some(7) }','assert_eq!(read_as_object(), Some(7));','helper_owner_call',True),
 ('intermediate-async','fn owner_value() -> Option<u32> { JsonValue::Object(7).as_object() }\nfn read_as_object() -> Option<u32> { assert_eq!({ let _ = async move { owner_value() }; 7 }, 7); Some(7) }','assert_eq!(read_as_object(), Some(7));',None,False),
 ('intermediate-mixed','fn owner_value() -> Option<u32> { JsonValue::Object(7).as_object() }\nfn read_as_object() -> Option<u32> { assert_eq!({ let _ = async move { owner_value() }; owner_value() }, Some(7)); Some(7) }','assert_eq!(read_as_object(), Some(7));',None,True),
 ('async-function-helper','async fn read_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }','let _ = read_as_object(); assert_eq!(7, 7);',None,False),
 ('async-function-intermediate','async fn owner_value() -> Option<u32> { JsonValue::Object(7).as_object() }\nfn read_as_object() -> Option<u32> { let _ = owner_value(); Some(7) }','assert_eq!(read_as_object(), Some(7));',None,False),
 ('async-function-operand','async fn read_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }','assert_eq!({ let _ = super::read_as_object(); 7 }, 7);',None,False),
 ('async-function-named-intermediate','async fn load_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }\nfn read_as_object() -> Option<u32> { let _ = load_as_object(); Some(7) }','assert_eq!(read_as_object(), Some(7));',None,False),
]
for label,helper,body,relation,reaches in stimuli:
 p=out/(label+'.rs');p.write_text(base+helper+'\n#[cfg(test)] mod tests { use super::*;\n#[test] fn subject() { '+body+' }\n}\n')
 cases.append({'path':str(p),'name':'subject','relation':relation,'native_reaches':reaches,'label':label})
(out/'cases.json').write_text(json.dumps(cases,indent=2)+'\n')
print(len(cases))
