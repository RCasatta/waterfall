//! # Integration testing
//!
//! This is not going to depend on LWK because we want to use this lib in LWK testing
//! Thus following tests aren't a proper wallet scan but they checks memory/db backend and also
//! mempool/confirmation result in receiving a payment

use waterfall::route::WaterfallResponse;

#[cfg(feature = "test_env")]
#[tokio::test]
async fn integration_memory() {
    let test_env = launch_memory().await;
    do_test(test_env).await;
}

#[cfg(all(feature = "test_env", feature = "db"))]
#[tokio::test]
async fn integration_db() {
    let tempdir = tempfile::TempDir::new().unwrap();
    let path = tempdir.path().to_path_buf();
    let exe = std::env::var("ELEMENTSD_EXEC").unwrap();
    let test_env = waterfall::test_env::launch(exe, Some(path)).await;
    do_test(test_env).await;
}

#[cfg(all(feature = "test_env", feature = "db"))]
async fn launch_memory() -> waterfall::test_env::TestEnv {
    let exe = std::env::var("ELEMENTSD_EXEC").unwrap();
    waterfall::test_env::launch(exe, None).await
}

#[cfg(all(feature = "test_env", not(feature = "db")))]
async fn launch_memory() -> waterfall::test_env::TestEnv {
    let exe = std::env::var("ELEMENTSD_EXEC").unwrap();
    waterfall::test_env::launch(exe).await
}

#[cfg(feature = "test_env")]
async fn do_test(test_env: waterfall::test_env::TestEnv) {
    use elements::{bitcoin::secp256k1, AddressParams};
    use elements_miniscript::{ConfidentialDescriptor, DescriptorPublicKey};
    use std::str::FromStr;
    let secp = secp256k1::Secp256k1::new();

    let bitcoin_desc = "elwpkh(tpubDC8msFGeGuwnKG9Upg7DM2b4DaRqg3CUZa5g8v2SRQ6K4NSkxUgd7HsL2XVWbVm39yBA4LAxysQAm397zwQSQoQgewGiYZqrA9DsP4zbQ1M/<0;1>/*)";
    let single_bitcoin_desc = bitcoin_desc.replace("<0;1>", "0");
    let blinding = "slip77(9c8e4f05c7711a98c838be228bcb84924d4570ca53f35fa1c793e58841d47023)";
    let desc_str = format!("ct({blinding},{single_bitcoin_desc})#qwqap8xk"); // we use a non-multipath to generate addresses
    let base_url = test_env.base_url();
    let client = reqwest::Client::new();
    let result = make_waterfall_req(&client, &base_url, &bitcoin_desc).await;
    assert_eq!(result.page, 0);
    assert_eq!(result.txs_seen.len(), 2);
    assert!(result.is_empty());

    let desc = ConfidentialDescriptor::<DescriptorPublicKey>::from_str(&desc_str).unwrap();
    let addr = desc
        .at_derivation_index(0)
        .unwrap()
        .address(&secp, &AddressParams::ELEMENTS)
        .unwrap();

    let txid = test_env.send_to(&addr, 10_000);

    tokio::time::sleep(std::time::Duration::from_secs(2)).await; // give some time to start the server, TODO should wait conditionally

    let result = make_waterfall_req(&client, &base_url, &bitcoin_desc).await;
    assert_eq!(result.page, 0);
    assert_eq!(result.txs_seen.len(), 2);
    assert!(!result.is_empty());
    assert_eq!(result.count_non_empty(), 1);
    let first = &result.txs_seen.iter().next().unwrap().1[0][0];
    assert_eq!(first.txid, txid);
    assert_eq!(first.height, 0);
    assert_eq!(first.block_hash, None);
    assert_eq!(first.block_timestamp, None);

    test_env.node_generate(1);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await; // give some time for scan to happen, TODO should wait conditionally

    let result = make_waterfall_req(&client, &base_url, &bitcoin_desc).await;
    assert_eq!(result.page, 0);
    assert_eq!(result.txs_seen.len(), 2);
    assert!(!result.is_empty());
    assert_eq!(result.count_non_empty(), 1);
    let first = &result.txs_seen.iter().next().unwrap().1[0][0];
    assert_eq!(first.txid, txid);
    assert_eq!(first.height, 3);
    assert!(first.block_hash.is_some());
    assert!(first.block_timestamp.is_some());

    test_env.shutdown().await;
    assert!(true);
}

async fn make_waterfall_req(
    client: &reqwest::Client,
    base_url: &str,
    desc: &str,
) -> WaterfallResponse {
    let descriptor_url = format!("{}/v1/waterfall", base_url);

    let response = client
        .get(&descriptor_url)
        .query(&[("descriptor", desc)])
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = response.text().await.unwrap();
    println!("{body}");
    serde_json::from_str(&body).unwrap()
}