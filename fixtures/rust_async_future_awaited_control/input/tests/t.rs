use fx::fetch_len;

#[tokio::test]
async fn fetch_len_awaited() {
    let _n = fetch_len("abc").await;
}
