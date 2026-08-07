use crate::common::*;

#[tokio::test]
async fn debug_list_sites_url() {
    let server = TestServer::new().await;
    let token = server
        .bootstrap_owner("admin", "correct horse battery staple")
        .await;
    let url1 = format!("{}/api/v1/sites/", server.base_url());
    let url2 = format!("{}/api/v1/sites", server.base_url());
    eprintln!("URL with slash: {url1}");
    eprintln!("URL without slash: {url2}");

    let resp1 = server.client().get(&url1).bearer_auth(&token).send().await.unwrap();
    eprintln!("with slash status: {}", resp1.status());
    eprintln!("with slash body: {}", resp1.text().await.unwrap_or_default());

    let resp2 = server.client().get(&url2).bearer_auth(&token).send().await.unwrap();
    eprintln!("without slash status: {}", resp2.status());
    eprintln!("without slash body: {}", resp2.text().await.unwrap_or_default());
}
