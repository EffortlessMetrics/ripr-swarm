import pathlib,json
root=pathlib.Path('/workspace/8004-repair/new-fixture')
(root/'input/src').mkdir(parents=True,exist_ok=True)
(root/'controls').mkdir(exist_ok=True)
repo=pathlib.Path('/workspace/ripr-swarm')
base=(repo/'fixtures/inherent_receiver_async_operand_helper/input/src/lib.rs').read_text().split('fn read_as_object()')[0]
review=pathlib.Path('/workspace/8004-repair/evidence/8004/independent-a2-review/findings')
rows=[]
def add(file,helpers,test,relation=None,native=False):
    src=base+helpers+'\n#[cfg(test)] mod tests { use super::*;\n#[test] fn subject() { '+test+' }\n}\n'
    (root/file).write_text(src)
    rows.append(dict(path='inherent_receiver_async_function/'+file,name='subject',relation=relation,native_reaches=native))
add('input/src/lib.rs','async fn read_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }','assert_eq!({ let _ = super::read_as_object(); 7 }, 7);')
add('controls/entry.rs','async fn read_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }','let _ = read_as_object(); assert_eq!(7, 7);')
add('controls/last-hop.rs','async fn load_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }\nfn read_as_object() -> Option<u32> { let _ = load_as_object(); Some(7) }','assert_eq!(read_as_object(), Some(7));')
add('controls/intermediate.rs','fn fetch_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }\nasync fn load_as_object() -> Option<u32> { fetch_as_object() }\nfn read_as_object() -> Option<u32> { let _ = load_as_object(); Some(7) }','assert_eq!(read_as_object(), Some(7));')
add('controls/sync-chain.rs','fn fetch_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }\nfn load_as_object() -> Option<u32> { fetch_as_object() }\nfn read_as_object() -> Option<u32> { load_as_object() }','assert_eq!(read_as_object(), Some(7));','helper_owner_call',True)
add('controls/mixed-helper.rs','async fn load_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }\nfn read_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }','let _ = load_as_object(); assert_eq!(read_as_object(), Some(7));','helper_owner_call',True)
add('controls/mixed-direct.rs','async fn read_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }','let _ = read_as_object(); assert_eq!(JsonValue::Object(7).as_object(), Some(7));','direct_owner_call',True)
add('controls/method-entry.rs','impl JsonValue { async fn read_as_object(&self) -> Option<u32> { let value = JsonValue::Object(7); value.as_object() } }','let value = JsonValue::Object(7); let _ = value.read_as_object(); assert_eq!(7, 7);')
add('controls/method-last-hop.rs','impl JsonValue { async fn load_as_object(&self) -> Option<u32> { let value = JsonValue::Object(7); value.as_object() } }\nfn read_as_object() -> Option<u32> { let value = JsonValue::Object(7); let _ = value.load_as_object(); Some(7) }','assert_eq!(read_as_object(), Some(7));')
add('controls/activated-unsupported.rs','async fn read_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }','use std::future::Future; let mut future = std::pin::pin!(read_as_object()); let mut context = std::task::Context::from_waker(std::task::Waker::noop()); assert_eq!(future.as_mut().poll(&mut context), std::task::Poll::Ready(Some(7)));',None,True)
add('controls/sync-with-dormant-block.rs','fn read_as_object() -> Option<u32> { let _ = async { JsonValue::Object(7).as_object() }; JsonValue::Object(7).as_object() }','assert_eq!(read_as_object(), Some(7));','helper_owner_call',True)
add('controls/sync-keyword.rs','fn read_as_object() -> Option<u32> { let _ = "async fn read_as_object"; JsonValue::Object(7).as_object() }','assert_eq!(read_as_object(), Some(7));','helper_owner_call',True)
add('controls/method-sync.rs','impl JsonValue { fn read_as_object(&self) -> Option<u32> { let value = JsonValue::Object(7); value.as_object() } }','let value = JsonValue::Object(7); assert_eq!(value.read_as_object(), Some(7));','helper_owner_call',True)
(root/'cases.json').write_text(json.dumps(rows,indent=2)+'\n')
