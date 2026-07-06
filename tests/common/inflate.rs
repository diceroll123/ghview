use ghview::app::App;
use ghview::data::{
    fetch_issues_with, fetch_pr_body_with, fetch_prs_with, fetch_repo_frontpage_with,
    fetch_repos_with, fetch_source_issues_with, fetch_source_prs_with,
};
use ghview::types::{Repo, RepoId, RepoSortKey, Source};

use super::builders;
use super::gh_mock::MockGh;

fn octo_repo_id() -> RepoId {
    RepoId::new("octo-org", "repo-charlie")
}

fn setup_octo_repo(app: &mut App) {
    app.sources = vec![Source::Org("octo-org".into())];
    app.source_state.select(Some(0));
    app.source_ctx.repos = vec![Repo {
        name: "repo-charlie".into(),
        ..Repo::default()
    }];
    app.source_ctx.repo_state.select(Some(0));
}

pub async fn app_with_repo_list() -> App {
    let mut app = builders::make_app();
    app.sources = vec![Source::Org("octo-org".into())];
    app.source_state.select(Some(0));

    let gh = MockGh::new().on_fixture(
        "orgs/octo-org/repos?per_page=100&page=1&sort=pushed&direction=desc",
        "repos_org.jsonl",
    );
    let repos = fetch_repos_with(
        &gh,
        &Source::Org("octo-org".into()),
        "octocat",
        100,
        1,
        RepoSortKey::default(),
    )
    .await
    .unwrap_or_default();

    app.source_ctx.repos = repos;
    app.source_ctx.repo_state.select(Some(0));
    app
}

pub async fn app_with_prs() -> App {
    let mut app = builders::make_app();
    setup_octo_repo(&mut app);

    let gh = MockGh::new().on_fixture(
        "repos/octo-org/repo-charlie/pulls?state=open&per_page=100&page=1&sort=created&direction=desc",
        "prs.jsonl",
    );
    let prs = fetch_prs_with(&gh, &octo_repo_id(), 100, 1)
        .await
        .unwrap_or_default();

    app.repo_ctx.prs_raw = prs.clone();
    app.repo_ctx.prs = prs;
    app.repo_ctx.pr_state.select(Some(0));
    app
}

pub async fn app_with_pr_detail() -> App {
    let mut app = app_with_prs().await;
    let pr_number = app.repo_ctx.prs.first().map(|pr| pr.number).unwrap_or(0);

    let endpoint = format!("repos/octo-org/repo-charlie/pulls/{pr_number}");
    let gh = MockGh::new().on_fixture(&endpoint, "pr_body.json");
    let (body, _, _, _, _, _) = fetch_pr_body_with(&gh, &octo_repo_id(), pr_number)
        .await
        .expect("fetch_pr_body_with should succeed against pr_body.json fixture");

    app.repo_ctx.pr_body = Some(body);
    app
}

pub async fn app_with_issues() -> App {
    let mut app = builders::make_app();
    setup_octo_repo(&mut app);

    let gh = MockGh::new().on_fixture(
        "repos/octo-org/repo-charlie/issues?state=open&per_page=100&page=1",
        "issues.jsonl",
    );
    let (issues, _has_more) = fetch_issues_with(&gh, &octo_repo_id(), 100, 1)
        .await
        .unwrap_or_default();

    app.repo_ctx.issues = issues;
    app.repo_ctx.issue_state.select(Some(0));
    app
}

pub async fn app_with_source_prs() -> App {
    let mut app = builders::make_app();
    app.sources = vec![Source::User("octocat".into())];
    app.source_state.select(Some(0));

    let gh = MockGh::new().on_fixture(
        "search/issues?q=is:pr+is:open+author:octocat&sort=created&order=desc&per_page=100&page=1",
        "source_prs.jsonl",
    );
    let prs = fetch_source_prs_with(&gh, "octocat", false, 100, 1)
        .await
        .unwrap_or_default();

    app.source_ctx.source_prs = prs;
    app.source_ctx.source_pr_state.select(Some(0));
    app
}

pub async fn app_with_source_issues() -> App {
    let mut app = builders::make_app();
    app.sources = vec![Source::Org("octo-org".into())];
    app.source_state.select(Some(0));

    let gh = MockGh::new().on_fixture(
        "search/issues?q=is:issue+is:open+org:octo-org&sort=created&order=desc&per_page=100&page=1",
        "source_issues.jsonl",
    );
    let issues = fetch_source_issues_with(&gh, "octo-org", true, 100, 1)
        .await
        .unwrap_or_default();

    app.source_ctx.source_issues = issues;
    app.source_ctx.source_issue_state.select(Some(0));
    app
}

pub async fn app_with_frontpage() -> App {
    let mut app = builders::make_app();
    setup_octo_repo(&mut app);

    let gh = MockGh::new()
        .on_fixture("repos/octo-org/repo-charlie", "repo_description.txt")
        .on_fixture("repos/octo-org/repo-charlie/readme", "readme.md");
    let (description, readme) = fetch_repo_frontpage_with(&gh, &octo_repo_id())
        .await
        .unwrap_or_default();

    app.repo_ctx.repo_frontpage = Some((description, readme));
    app
}
