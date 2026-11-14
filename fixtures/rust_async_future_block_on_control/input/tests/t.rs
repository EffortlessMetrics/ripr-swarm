use fx::fetch_len;

#[test]
fn fetch_len_blocked() {
    let _n = futures::executor::block_on(fetch_len("abc"));
}
