mod common;

use common::gh_mock::MockGh;
use ghview::config::SourcesConfig;
use ghview::data::*;
use ghview::types::{CheckStatus, MergeableState, RepoId, RepoSortKey, ReviewStatus, Source};

#[tokio::test]
async fn fetch_user_success() {
    let gh = MockGh::new().on("user", "octocat\n");
    let result = fetch_user_with(&gh).await.unwrap();
    assert_eq!(result, "octocat");
}

#[tokio::test]
async fn fetch_user_error() {
    let gh = MockGh::new().on_err("user", "not authenticated");
    let result = fetch_user_with(&gh).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn fetch_sources_small_orgs() {
    let cfg = SourcesConfig {
        auto_fetch_orgs: true,
        include_self: true,
        ..Default::default()
    };
    let gh = MockGh::new()
        .on("user", "octocat\n")
        .on_fixture("user/memberships/orgs?per_page=100&page=1", "orgs.txt");
    let (sources, current_user) = fetch_sources_with(&gh, &cfg).await.unwrap();
    assert_eq!(current_user, "octocat");
    assert_eq!(sources.len(), 4);
    assert!(matches!(sources[0], Source::User(ref s) if s == "octocat"));
    assert!(matches!(sources[1], Source::Org(ref s) if s == "acme-tools"));
    assert!(matches!(sources[2], Source::Org(ref s) if s == "example-labs"));
    assert!(matches!(sources[3], Source::Org(ref s) if s == "octo-org"));
}

#[tokio::test]
async fn fetch_sources_pagination() {
    let cfg = SourcesConfig {
        auto_fetch_orgs: true,
        include_self: false,
        ..Default::default()
    };
    let gh = MockGh::new()
        .on("user", "octocat\n")
        .on_fixture(
            "user/memberships/orgs?per_page=100&page=1",
            "orgs_page_full.txt",
        )
        .on("user/memberships/orgs?per_page=100&page=2", "org-101\n");
    let (sources, current_user) = fetch_sources_with(&gh, &cfg).await.unwrap();
    assert_eq!(current_user, "octocat");
    assert_eq!(sources.len(), 101);
}

#[tokio::test]
async fn fetch_repos_org() {
    let gh = MockGh::new().on_fixture(
        "orgs/octo-org/repos?per_page=30&page=1&sort=pushed&direction=desc",
        "repos_org.jsonl",
    );
    let repos = fetch_repos_with(
        &gh,
        &Source::Org("octo-org".into()),
        "octocat",
        30,
        1,
        RepoSortKey::RecentlyUpdated,
    )
    .await
    .unwrap();
    assert_eq!(repos.len(), 21);
    let octo_org_repo = repos.iter().find(|r| r.name == "octo-org").unwrap();
    assert_eq!(octo_org_repo.stars, 21433);
}

#[tokio::test]
async fn fetch_repos_user_current() {
    let gh = MockGh::new().on_fixture(
        "user/repos?per_page=30&page=1&sort=pushed&direction=desc",
        "repos_user.jsonl",
    );
    let repos = fetch_repos_with(
        &gh,
        &Source::User("octocat".into()),
        "octocat",
        30,
        1,
        RepoSortKey::RecentlyUpdated,
    )
    .await
    .unwrap();
    let expected_count = common::fixture("repos_user.jsonl")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    assert_eq!(repos.len(), expected_count);
}

#[tokio::test]
async fn fetch_repos_owner_filtering() {
    let inline_jsonl = r#"{"name":"a","owner_login":"octocat"}
{"name":"b","owner_login":"someone-else"}"#;
    let gh = MockGh::new().on(
        "users/octocat/repos?per_page=30&page=1&sort=pushed&direction=desc",
        inline_jsonl,
    );
    let repos = fetch_repos_with(
        &gh,
        &Source::User("octocat".into()),
        "someoneelse",
        30,
        1,
        RepoSortKey::RecentlyUpdated,
    )
    .await
    .unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].name, "a");
}

#[tokio::test]
async fn fetch_prs_happy_path() {
    let gh = MockGh::new().on_fixture(
        "repos/octo-org/repo-charlie/pulls?state=open&per_page=30&page=1&sort=created&direction=desc",
        "prs.jsonl",
    );
    let prs = fetch_prs_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 30, 1)
        .await
        .unwrap();
    assert_eq!(prs.len(), 30);
    let pr_2628 = prs.iter().find(|p| p.number == 2628).unwrap();
    assert_eq!(pr_2628.title, "Fix panic when list is empty");
    assert!(!pr_2628.draft);
    let pr_2606 = prs.iter().find(|p| p.number == 2606).unwrap();
    assert!(pr_2606.draft);
}

#[tokio::test]
async fn fetch_prs_malformed_tolerance() {
    let gh = MockGh::new().on_fixture(
        "repos/octo-org/repo-alpha/pulls?state=open&per_page=30&page=1&sort=created&direction=desc",
        "prs_malformed.jsonl",
    );
    let prs = fetch_prs_with(&gh, &RepoId::new("octo-org", "repo-alpha"), 30, 1)
        .await
        .unwrap();
    assert_eq!(prs.len(), 3);
}

#[tokio::test]
async fn fetch_prs_all_malformed() {
    let gh = MockGh::new().on_fixture(
        "repos/octo-org/repo-x/pulls?state=open&per_page=30&page=1&sort=created&direction=desc",
        "prs_all_malformed.jsonl",
    );
    let result = fetch_prs_with(&gh, &RepoId::new("octo-org", "repo-x"), 30, 1).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn fetch_prs_runner_error() {
    let gh = MockGh::new().on_err(
        "repos/octo-org/repo-charlie/pulls?state=open&per_page=30&page=1&sort=created&direction=desc",
        "boom",
    );
    let result = fetch_prs_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 30, 1).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn fetch_source_prs_author() {
    let gh = MockGh::new().on_fixture(
        "search/issues?q=is:pr+is:open+author:octocat&sort=created&order=desc&per_page=30&page=1",
        "source_prs.jsonl",
    );
    let prs = fetch_source_prs_with(&gh, "octocat", false, 30, 1)
        .await
        .unwrap();
    assert_eq!(prs.len(), 10);
    let pr_3460 = prs.iter().find(|p| p.number == 3460).unwrap();
    assert_eq!(pr_3460.repo, "repo-zulu-2");
    assert_eq!(pr_3460.repo_owner, "user-28");
}

#[tokio::test]
async fn fetch_source_prs_org() {
    let inline_jsonl = r#"{"author":"octo-org","comments":0,"created_at":"2026-01-15T11:15:00Z","labels":[],"number":999,"repo":"test-repo","repo_owner":"octo-org","state":"open","title":"Test PR","updated_at":"2026-01-15T11:15:00Z","url":"https://github.com/octo-org/test-repo/pull/999"}"#;
    let gh = MockGh::new().on(
        "search/issues?q=is:pr+is:open+org:octo-org&sort=created&order=desc&per_page=30&page=1",
        inline_jsonl,
    );
    let prs = fetch_source_prs_with(&gh, "octo-org", true, 30, 1)
        .await
        .unwrap();
    assert_eq!(prs.len(), 1);
}

#[tokio::test]
async fn fetch_review_status_approved() {
    let endpoint = "repos/octo-org/repo-charlie/pulls/7/reviews?per_page=100";
    let gh = MockGh::new().on(endpoint, "APPROVED\nAPPROVED");
    let status = fetch_review_status_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 7).await;
    assert_eq!(status, ReviewStatus::Approved);
}

#[tokio::test]
async fn fetch_review_status_changes_requested() {
    let endpoint = "repos/octo-org/repo-charlie/pulls/7/reviews?per_page=100";
    let gh = MockGh::new().on(endpoint, "APPROVED\nCHANGES_REQUESTED\nCOMMENTED");
    let status = fetch_review_status_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 7).await;
    assert_eq!(status, ReviewStatus::ChangesRequested);
}

#[tokio::test]
async fn fetch_review_status_pending() {
    let endpoint = "repos/octo-org/repo-charlie/pulls/7/reviews?per_page=100";
    let gh = MockGh::new().on(endpoint, "COMMENTED\nCOMMENTED");
    let status = fetch_review_status_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 7).await;
    assert_eq!(status, ReviewStatus::Pending);
}

#[tokio::test]
async fn fetch_review_status_no_reviews() {
    let endpoint = "repos/octo-org/repo-charlie/pulls/7/reviews?per_page=100";
    let gh = MockGh::new().on(endpoint, "");
    let status = fetch_review_status_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 7).await;
    assert_eq!(status, ReviewStatus::Pending);
}

#[tokio::test]
async fn fetch_review_status_runner_error() {
    let endpoint = "repos/octo-org/repo-charlie/pulls/7/reviews?per_page=100";
    let gh = MockGh::new().on_err(endpoint, "boom");
    let status = fetch_review_status_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 7).await;
    assert_eq!(status, ReviewStatus::Unknown);
}

#[tokio::test]
async fn fetch_check_runs_fixture_joined() {
    let sha = "abc123";
    let gh = MockGh::new()
        .on_fixture(
            &format!("repos/octo-org/repo-charlie/commits/{}/check-runs", sha),
            "check_runs.json",
        )
        .on_fixture(
            &format!("repos/octo-org/repo-charlie/actions/runs?head_sha={}", sha),
            "workflow_runs.json",
        );
    let check_runs = fetch_check_runs_with(&gh, &RepoId::new("octo-org", "repo-charlie"), sha)
        .await
        .len();
    // Count "id": occurrences in check_runs.json
    let fixture_text = common::fixture("check_runs.json");
    let expected_count = fixture_text.matches("\"id\":").count();
    assert_eq!(check_runs, expected_count);

    let check_runs =
        fetch_check_runs_with(&gh, &RepoId::new("octo-org", "repo-charlie"), sha).await;
    let run_100001 = check_runs.iter().find(|r| r.id == 100001).unwrap();
    assert_eq!(run_100001.status, CheckStatus::Passing);
    assert_eq!(
        run_100001.name,
        "Continuous Integration / required (pull_request)"
    );

    let run_100003 = check_runs.iter().find(|r| r.id == 100003).unwrap();
    assert_eq!(run_100003.name, "codecov/patch");
}

#[tokio::test]
async fn fetch_check_runs_status_mapping() {
    let sha = "def456";
    let runs_json = r#"[{"id":1,"name":"a","url":"","suite_id":1,"s":"failing"},{"id":2,"name":"b","url":"","suite_id":1,"s":"pending"},{"id":3,"name":"c","url":"","suite_id":1,"s":"unknown"},{"id":4,"name":"d","url":"","suite_id":1,"s":"passing"}]"#;
    let workflows_json = "[]";
    let gh = MockGh::new()
        .on(
            &format!("repos/octo-org/repo-charlie/commits/{}/check-runs", sha),
            runs_json,
        )
        .on(
            &format!("repos/octo-org/repo-charlie/actions/runs?head_sha={}", sha),
            workflows_json,
        );
    let check_runs =
        fetch_check_runs_with(&gh, &RepoId::new("octo-org", "repo-charlie"), sha).await;
    assert_eq!(check_runs.len(), 4);
    assert_eq!(check_runs[0].status, CheckStatus::Failing);
    assert_eq!(check_runs[1].status, CheckStatus::Pending);
    assert_eq!(check_runs[2].status, CheckStatus::Unknown);
    assert_eq!(check_runs[3].status, CheckStatus::Passing);
}

#[tokio::test]
async fn fetch_check_runs_empty_sha_short_circuit() {
    let gh = MockGh::new();
    let check_runs = fetch_check_runs_with(&gh, &RepoId::new("o", "r"), "").await;
    assert_eq!(check_runs.len(), 0);
}

#[tokio::test]
async fn fetch_check_runs_runner_error_soft_failure() {
    let sha = "err123";
    let gh = MockGh::new()
        .on_err(
            &format!("repos/octo-org/repo-charlie/commits/{}/check-runs", sha),
            "boom runs",
        )
        .on_err(
            &format!("repos/octo-org/repo-charlie/actions/runs?head_sha={}", sha),
            "boom workflows",
        );
    let check_runs =
        fetch_check_runs_with(&gh, &RepoId::new("octo-org", "repo-charlie"), sha).await;
    assert_eq!(check_runs.len(), 0);
}

#[tokio::test]
async fn fetch_rate_limit_success() {
    let gh = MockGh::new().on_fixture("rate_limit", "rate_limit.txt");
    let result = fetch_rate_limit_with(&gh).await.unwrap();
    assert_eq!(result, (4321, 5000));
}

#[tokio::test]
async fn fetch_rate_limit_error() {
    let gh = MockGh::new().on_err("rate_limit", "boom");
    let result = fetch_rate_limit_with(&gh).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn fetch_diff_success() {
    let diff_content = common::fixture("diff.txt");
    let gh = MockGh::new().on("pr diff 5 -R octo-org/repo-charlie", &diff_content);
    let result = fetch_diff_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 5)
        .await
        .unwrap();
    assert_eq!(result, diff_content);
}

#[tokio::test]
async fn fetch_repo_frontpage_success() {
    let gh = MockGh::new()
        .on_fixture("repos/octo-org/repo-charlie", "repo_description.txt")
        .on_fixture("repos/octo-org/repo-charlie/readme", "readme.md");
    let (description, readme) =
        fetch_repo_frontpage_with(&gh, &RepoId::new("octo-org", "repo-charlie"))
            .await
            .unwrap();
    assert_eq!(description, common::fixture("repo_description.txt").trim());
    assert_eq!(readme, common::fixture("readme.md"));
}

#[tokio::test]
async fn fetch_issues_success() {
    let gh = MockGh::new().on_fixture(
        "repos/octo-org/repo-charlie/issues?state=open&per_page=30&page=1",
        "issues.jsonl",
    );
    let (issues, has_more) =
        fetch_issues_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 30, 1)
            .await
            .unwrap();
    assert_eq!(issues.len(), 11);
    assert!(has_more);
    // Issue 2628 is a PR (is_pr:true) and should be filtered out
    let issue_2628 = issues.iter().find(|i| i.number == 2628);
    assert!(issue_2628.is_none());
    // Issue 2610 is an actual issue (is_pr:false) and should be present
    let issue_2610 = issues.iter().find(|i| i.number == 2610).unwrap();
    assert_eq!(issue_2610.title, "Fix panic when list is empty");
}

#[tokio::test]
async fn fetch_issues_has_more_false() {
    let inline_jsonl = r#"{"author":"user-1","created_at":"2026-01-15T11:15:00Z","is_pr":false,"labels":[],"number":999,"title":"Test Issue","url":"https://github.com/octo-org/repo-test/issues/999"}
{"author":"user-2","created_at":"2026-01-14T10:00:00Z","is_pr":false,"labels":[],"number":998,"title":"Another Issue","url":"https://github.com/octo-org/repo-test/issues/998"}"#;
    let gh = MockGh::new().on(
        "repos/octo-org/repo-charlie/issues?state=open&per_page=30&page=2",
        inline_jsonl,
    );
    let (issues, has_more) =
        fetch_issues_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 30, 2)
            .await
            .unwrap();
    assert_eq!(issues.len(), 2);
    assert!(!has_more);
}

#[tokio::test]
async fn fetch_source_issues_org() {
    let gh = MockGh::new().on_fixture(
        "search/issues?q=is:issue+is:open+org:octo-org&sort=created&order=desc&per_page=30&page=1",
        "source_issues.jsonl",
    );
    let issues = fetch_source_issues_with(&gh, "octo-org", true, 30, 1)
        .await
        .unwrap();
    assert_eq!(issues.len(), 30);
    let issue_184 = issues.iter().find(|i| i.number == 184).unwrap();
    assert_eq!(issue_184.repo, "octo-org-image");
    assert_eq!(issue_184.repo_owner, "octo-org");
}

#[tokio::test]
async fn fetch_issue_body_success() {
    let gh = MockGh::new().on_fixture("repos/octo-org/repo-charlie/issues/2461", "issue_body.md");
    let body = fetch_issue_body_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 2461)
        .await
        .unwrap();
    assert_eq!(body, common::fixture("issue_body.md").trim());
}

#[tokio::test]
async fn fetch_pr_body_success() {
    let gh = MockGh::new().on_fixture("repos/octo-org/repo-charlie/pulls/2628", "pr_body.json");
    let (body, mergeable_state, additions, deletions, head_sha, auto_merge) =
        fetch_pr_body_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 2628)
            .await
            .unwrap();
    assert!(body.starts_with("Closes #2526."));
    assert_eq!(mergeable_state, MergeableState::Blocked);
    assert_eq!(additions, 48);
    assert_eq!(deletions, 1);
    assert_eq!(head_sha, "b6589fc6ab0dc82cf12099d1c2d40ab994e8410c");
    assert!(!auto_merge);
}

#[tokio::test]
async fn fetch_pr_body_unknown_mergeable_state() {
    let inline_json = r#"{"additions":1,"auto_merge":false,"body":"x","deletions":1,"head_sha":"abc","mergeable_state":"some_bogus_value"}"#;
    let gh = MockGh::new().on("repos/octo-org/repo-charlie/pulls/999", inline_json);
    let (_, mergeable_state, _, _, _, _) =
        fetch_pr_body_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 999)
            .await
            .unwrap();
    assert_eq!(mergeable_state, MergeableState::Unknown);
}

#[tokio::test]
async fn fetch_viewer_approved_true() {
    let endpoint = "repos/octo-org/repo-charlie/pulls/7/reviews?per_page=100";
    let gh = MockGh::new().on(endpoint, "true\n");
    let result =
        fetch_viewer_approved_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 7, "alice").await;
    assert!(result);
}

#[tokio::test]
async fn fetch_viewer_approved_false() {
    let endpoint = "repos/octo-org/repo-charlie/pulls/7/reviews?per_page=100";
    let gh = MockGh::new().on(endpoint, "false\n");
    let result =
        fetch_viewer_approved_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 7, "alice").await;
    assert!(!result);
}

#[tokio::test]
async fn fetch_viewer_approved_runner_error() {
    let endpoint = "repos/octo-org/repo-charlie/pulls/7/reviews?per_page=100";
    let gh = MockGh::new().on_err(endpoint, "boom");
    let result =
        fetch_viewer_approved_with(&gh, &RepoId::new("octo-org", "repo-charlie"), 7, "alice").await;
    assert!(!result);
}

#[tokio::test]
async fn fetch_viewer_permission_success() {
    let gh = MockGh::new().on_fixture("repos/octo-org/repo-charlie", "viewer_permission.json");
    let (can_push, allow_auto_merge) =
        fetch_viewer_permission_with(&gh, &RepoId::new("octo-org", "repo-charlie")).await;
    assert!(can_push);
    assert!(!allow_auto_merge);
}

#[tokio::test]
async fn fetch_viewer_permission_error() {
    let gh = MockGh::new().on_err("repos/octo-org/repo-charlie", "boom");
    let (can_push, allow_auto_merge) =
        fetch_viewer_permission_with(&gh, &RepoId::new("octo-org", "repo-charlie")).await;
    assert!(!can_push);
    assert!(!allow_auto_merge);
}
