mod common;

use ghview::data::fetch_user_with;

#[test]
fn fixture_loader_reads_user_login() {
    let body = common::fixture("user_login.txt");
    assert_eq!(body.trim(), "octocat");
}

#[tokio::test]
async fn mock_gh_serves_registered_endpoint() {
    let mock = common::gh_mock::MockGh::new().on("user", "octocat\n");
    let login = fetch_user_with(&mock).await.expect("fetch_user_with ok");
    assert_eq!(login, "octocat");
}
