use observation_verified_call_deletion_fixture::{Cache, store_result};

#[test]
fn store_result_inserts_result_key() {
    let mut cache = Cache::new();
    store_result(&mut cache, 42);
    assert!(cache.inserted.iter().any(|entry| entry.contains("result_key")));
}
