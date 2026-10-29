use super::*;

#[test]
fn a_redirect_off_https_or_past_the_hop_limit_is_not_followed() {
    let https = url::Url::parse("https://objects.githubusercontent.com/a").expect("url");
    let http = url::Url::parse("http://objects.githubusercontent.com/a").expect("url");
    assert_eq!(refuse_redirect(&https, 0), None);
    assert_eq!(refuse_redirect(&https, 4), None);
    assert!(refuse_redirect(&https, 5).is_some(), "a sixth hop");
    assert!(refuse_redirect(&http, 0).is_some());
}

#[tokio::test]
async fn the_production_fetcher_refuses_a_plain_http_address() {
    let url = url::Url::parse("http://127.0.0.1:9/manifest.json").expect("url");
    let error = HttpFetcher::new(&ProxyChoice::Direct)
        .fetch(&url, 1024)
        .await
        .expect_err("plain http");
    assert!(error.to_string().contains("not https"), "{error}");
}

/// What a check sends: `rCommander/<version>` and nothing that names the installation.
#[test]
fn the_user_agent_names_the_program_and_its_version_only() {
    assert_eq!(
        user_agent(),
        format!("rCommander/{}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn the_proxy_setting_reads_empty_direct_and_a_plain_address() {
    assert_eq!(ProxyChoice::parse(""), Ok(ProxyChoice::System));
    assert_eq!(ProxyChoice::parse("direct"), Ok(ProxyChoice::Direct));
    assert_eq!(
        ProxyChoice::parse("http://proxy.example:3128"),
        Ok(ProxyChoice::Address("http://proxy.example:3128".to_owned()))
    );
    // A login in the address is refused: the setting never holds a secret.
    assert!(ProxyChoice::parse("http://user:secret@proxy.example:3128").is_err());
    assert!(ProxyChoice::parse("socks5://proxy.example:1080").is_err());
    assert!(ProxyChoice::parse("http://proxy.example:3128/path?x=1").is_err());
}

#[tokio::test]
async fn the_memory_fetcher_answers_404_for_what_it_does_not_hold_and_bounds_the_body() {
    let fetcher = MemoryFetcher::new();
    fetcher.serve("https://example.test/a", b"abc".to_vec());
    let known = url::Url::parse("https://example.test/a").expect("url");
    let unknown = url::Url::parse("https://example.test/b").expect("url");
    assert_eq!(
        fetcher.fetch(&known, 10).await.expect("a"),
        Some(b"abc".to_vec())
    );
    assert_eq!(fetcher.fetch(&unknown, 10).await.expect("b"), None);
    let error = fetcher.fetch(&known, 2).await.expect_err("too large");
    assert!(error.downcast_ref::<BodyTooLarge>().is_some(), "{error}");
    assert_eq!(fetcher.requests().len(), 3);
}

#[tokio::test]
async fn a_memory_download_comes_in_several_chunks() {
    let fetcher = MemoryFetcher::new();
    fetcher.serve("https://example.test/a", vec![1_u8; 20]);
    let url = url::Url::parse("https://example.test/a").expect("url");
    let mut download = fetcher.open(&url).await.expect("open");
    assert_eq!(download.length, Some(20));
    let mut chunks = 0;
    while let Some(chunk) = download.body.next_chunk().await.expect("chunk") {
        assert!(!chunk.is_empty());
        chunks += 1;
    }
    assert!(chunks > 1);
}
