mod common;

use common::builders::{make_app, make_issue, make_pr_numbered, make_repo, setup_selected_repo};
use ghview::types::{
    CheckRun, CheckStatus, DataMsg, LoadingKind, MergeableState, RepoId, ReviewStatus, Source,
};

#[tokio::test]
async fn sources_selects_first_and_sets_current_user() {
    let mut app = make_app();
    app.handle_data(DataMsg::Sources {
        sources: vec![Source::User("owner".into())],
        current_user: "me".into(),
    });

    assert_eq!(app.current_user, Some("me".to_string()));
    assert_eq!(app.source_state.selected(), Some(0));
}

#[tokio::test]
async fn repos_accept_caches_and_applies() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.handle_data(DataMsg::Repos {
        owner: "owner".into(),
        repos: vec![make_repo("repo"), make_repo("zeta")],
        has_more: true,
    });

    assert_eq!(app.source_ctx.repos.len(), 2);
    assert_eq!(app.source_ctx.repos_pagination.page, 1);
    assert!(app.source_ctx.repos_pagination.has_more);
    assert!(
        app.repo_cache
            .contains_key(&("owner".to_string(), app.repo_sort_key))
    );
}

#[test]
fn repos_stale_owner_ignored() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.handle_data(DataMsg::Repos {
        owner: "other".into(),
        repos: vec![make_repo("other")],
        has_more: false,
    });

    assert!(app.repo_cache.is_empty());
    assert_eq!(app.source_ctx.repos.len(), 1);
}

#[test]
fn more_repos_extends_and_finishes_pagination() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.source_ctx.repos_pagination.has_more = true;
    app.source_ctx.repos_pagination.fetching_more = true;
    app.loading = Some(LoadingKind::Repos);

    app.handle_data(DataMsg::MoreRepos {
        owner: "owner".into(),
        repos: vec![make_repo("extra")],
        has_more: false,
    });

    assert_eq!(app.source_ctx.repos.len(), 2);
    assert!(!app.source_ctx.repos_pagination.has_more);
    assert!(!app.source_ctx.repos_pagination.fetching_more);
    assert!(app.loading.is_none());
    assert!(
        app.repo_cache
            .contains_key(&("owner".to_string(), app.repo_sort_key))
    );
}

#[test]
fn more_repos_stale_owner_ignored() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.handle_data(DataMsg::MoreRepos {
        owner: "other".into(),
        repos: vec![make_repo("extra")],
        has_more: false,
    });

    assert_eq!(app.source_ctx.repos.len(), 1);
    assert!(app.repo_cache.is_empty());
}

#[tokio::test]
async fn prs_current_repo_applies_and_caches() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.loading = Some(LoadingKind::Prs);

    app.handle_data(DataMsg::Prs {
        repo: RepoId::new("owner", "repo"),
        prs: vec![make_pr_numbered(1)],
        has_more: false,
    });

    assert_eq!(app.repo_ctx.prs_raw.len(), 1);
    assert_eq!(app.repo_ctx.prs.len(), 1);
    assert_eq!(app.repo_ctx.pr_state.selected(), Some(0));
    assert!(app.pr_cache.contains_key("owner/repo"));
    assert!(app.loading.is_none());
}

#[test]
fn prs_other_repo_only_caches() {
    let mut app = make_app();

    app.handle_data(DataMsg::Prs {
        repo: RepoId::new("owner", "repo"),
        prs: vec![make_pr_numbered(1)],
        has_more: false,
    });

    assert!(app.pr_cache.contains_key("owner/repo"));
    assert!(app.repo_ctx.prs_raw.is_empty());
}

#[test]
fn more_prs_extends_and_rebuilds() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.repo_ctx.prs_raw = vec![make_pr_numbered(1)];
    app.repo_ctx.prs = vec![make_pr_numbered(1)];
    app.loading = Some(LoadingKind::Prs);

    app.handle_data(DataMsg::MorePrs {
        repo: RepoId::new("owner", "repo"),
        prs: vec![make_pr_numbered(2)],
        has_more: true,
    });

    assert_eq!(app.repo_ctx.prs_raw.len(), 2);
    assert_eq!(app.repo_ctx.prs.len(), 2);
    assert!(app.repo_ctx.prs_pagination.has_more);
    assert!(app.loading.is_none());
}

#[test]
fn more_prs_stale_repo_ignored() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.handle_data(DataMsg::MorePrs {
        repo: RepoId::new("owner", "otherrepo"),
        prs: vec![make_pr_numbered(2)],
        has_more: false,
    });

    assert!(app.repo_ctx.prs_raw.is_empty());
}

#[test]
fn review_status_current_repo_updates_statuses() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.handle_data(DataMsg::ReviewStatus {
        pr: RepoId::new("owner", "repo").pr(7),
        status: ReviewStatus::Approved,
    });

    assert_eq!(
        app.repo_ctx.review_statuses.get(&7),
        Some(&ReviewStatus::Approved)
    );
}

#[test]
fn review_status_other_repo_not_applied() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.handle_data(DataMsg::ReviewStatus {
        pr: RepoId::new("owner", "otherrepo").pr(7),
        status: ReviewStatus::Approved,
    });

    assert!(app.repo_ctx.review_statuses.is_empty());
}

fn run(status: CheckStatus) -> CheckRun {
    CheckRun {
        id: 1,
        name: "ci".into(),
        url: String::new(),
        status,
    }
}

#[test]
fn check_runs_failing_precedence_and_sorted_first() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.repo_ctx.prs_raw = vec![make_pr_numbered(1)];
    app.repo_ctx.prs = vec![make_pr_numbered(1)];
    app.repo_ctx.pr_state.select(Some(0));

    app.handle_data(DataMsg::CheckRuns {
        pr: RepoId::new("owner", "repo").pr(1),
        runs: vec![run(CheckStatus::Passing), run(CheckStatus::Failing)],
    });

    assert_eq!(
        app.repo_ctx
            .check_summary_cache
            .get(&RepoId::new("owner", "repo").pr(1)),
        Some(&CheckStatus::Failing)
    );
    assert_eq!(
        app.repo_ctx.check_runs.as_ref().unwrap()[0].status,
        CheckStatus::Failing
    );
}

#[test]
fn check_runs_pending_precedence() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.repo_ctx.prs_raw = vec![make_pr_numbered(1)];
    app.repo_ctx.prs = vec![make_pr_numbered(1)];
    app.repo_ctx.pr_state.select(Some(0));

    app.handle_data(DataMsg::CheckRuns {
        pr: RepoId::new("owner", "repo").pr(1),
        runs: vec![run(CheckStatus::Passing), run(CheckStatus::Pending)],
    });

    assert_eq!(
        app.repo_ctx
            .check_summary_cache
            .get(&RepoId::new("owner", "repo").pr(1)),
        Some(&CheckStatus::Pending)
    );
}

#[test]
fn check_runs_all_passing_summary() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.repo_ctx.prs_raw = vec![make_pr_numbered(1)];
    app.repo_ctx.prs = vec![make_pr_numbered(1)];
    app.repo_ctx.pr_state.select(Some(0));

    app.handle_data(DataMsg::CheckRuns {
        pr: RepoId::new("owner", "repo").pr(1),
        runs: vec![run(CheckStatus::Passing), run(CheckStatus::Passing)],
    });

    assert_eq!(
        app.repo_ctx
            .check_summary_cache
            .get(&RepoId::new("owner", "repo").pr(1)),
        Some(&CheckStatus::Passing)
    );
}

#[test]
fn check_runs_empty_summary_unknown() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.repo_ctx.prs_raw = vec![make_pr_numbered(1)];
    app.repo_ctx.prs = vec![make_pr_numbered(1)];
    app.repo_ctx.pr_state.select(Some(0));

    app.handle_data(DataMsg::CheckRuns {
        pr: RepoId::new("owner", "repo").pr(1),
        runs: vec![],
    });

    assert_eq!(
        app.repo_ctx
            .check_summary_cache
            .get(&RepoId::new("owner", "repo").pr(1)),
        Some(&CheckStatus::Unknown)
    );
    assert!(app.repo_ctx.check_runs.as_ref().unwrap().is_empty());
}

#[test]
fn check_runs_stale_repo_ignored() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.repo_ctx.prs_raw = vec![make_pr_numbered(1)];
    app.repo_ctx.prs = vec![make_pr_numbered(1)];
    app.repo_ctx.pr_state.select(Some(0));

    app.handle_data(DataMsg::CheckRuns {
        pr: RepoId::new("owner", "otherrepo").pr(1),
        runs: vec![run(CheckStatus::Passing)],
    });

    assert!(app.repo_ctx.check_summary_cache.is_empty());
    assert!(app.repo_ctx.check_runs.is_none());
}

#[test]
fn diff_content_sets_diff_view() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.repo_ctx.prs_raw = vec![make_pr_numbered(1)];
    app.repo_ctx.prs = vec![make_pr_numbered(1)];
    app.repo_ctx.pr_state.select(Some(0));
    app.loading = Some(LoadingKind::Prs);

    app.handle_data(DataMsg::DiffContent {
        pr: RepoId::new("owner", "repo").pr(1),
        title: "diff".into(),
        content: "a\nb".into(),
    });

    assert!(app.repo_ctx.diff_view.is_some());
    let diff = app.repo_ctx.diff_view.as_ref().unwrap();
    assert_eq!(diff.title, "diff");
    assert_eq!(diff.lines.len(), 2);
    assert_eq!(diff.lines[1], "b");
    assert!(app.loading.is_none());
}

#[test]
fn diff_content_stale_repo_ignored() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.repo_ctx.prs_raw = vec![make_pr_numbered(1)];
    app.repo_ctx.prs = vec![make_pr_numbered(1)];
    app.repo_ctx.pr_state.select(Some(0));

    app.handle_data(DataMsg::DiffContent {
        pr: RepoId::new("owner", "otherrepo").pr(1),
        title: "diff".into(),
        content: "a\nb".into(),
    });

    assert!(app.repo_ctx.diff_view.is_none());
}

#[test]
fn pr_body_sets_body_and_propagates_additions() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.repo_ctx.prs_raw = vec![make_pr_numbered(1)];
    app.repo_ctx.prs = vec![make_pr_numbered(1)];
    app.repo_ctx.pr_state.select(Some(0));

    app.handle_data(DataMsg::PrBody {
        pr: RepoId::new("owner", "repo").pr(1),
        body: "hello".into(),
        mergeable_state: MergeableState::Clean,
        additions: 10,
        deletions: 3,
    });

    assert_eq!(app.repo_ctx.pr_body, Some("hello".to_string()));
    assert_eq!(
        app.repo_ctx
            .mergeable_states
            .get(&RepoId::new("owner", "repo").pr(1)),
        Some(&MergeableState::Clean)
    );
    assert_eq!(app.repo_ctx.prs_raw[0].additions, 10);
    assert_eq!(app.repo_ctx.prs_raw[0].deletions, 3);
    assert_eq!(app.repo_ctx.prs[0].additions, 10);
}

#[test]
fn pr_body_stale_repo_ignored() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.repo_ctx.prs_raw = vec![make_pr_numbered(1)];
    app.repo_ctx.prs = vec![make_pr_numbered(1)];
    app.repo_ctx.pr_state.select(Some(0));

    app.handle_data(DataMsg::PrBody {
        pr: RepoId::new("owner", "otherrepo").pr(1),
        body: "hello".into(),
        mergeable_state: MergeableState::Clean,
        additions: 10,
        deletions: 3,
    });

    assert!(app.repo_ctx.pr_body.is_none());
    assert!(app.repo_ctx.mergeable_states.is_empty());
    assert_eq!(app.repo_ctx.prs_raw[0].additions, 0);
}

#[test]
fn pr_body_updates_source_pr_in_pr_list_view() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.repos_view = ghview::types::ReposView::PrList;

    let mut spr = make_pr_numbered(1);
    spr.repo = "repo".into();
    spr.repo_owner = "owner".into();

    app.source_ctx.source_prs = vec![spr];
    app.source_ctx.source_pr_state.select(Some(0));

    app.handle_data(DataMsg::PrBody {
        pr: RepoId::new("owner", "repo").pr(1),
        body: "b".into(),
        mergeable_state: MergeableState::Clean,
        additions: 5,
        deletions: 2,
    });

    assert_eq!(app.source_ctx.source_prs[0].additions, 5);
    assert_eq!(app.source_ctx.source_prs[0].deletions, 2);
}

#[test]
fn repo_frontpage_current_repo_sets_frontpage() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.loading = Some(LoadingKind::Frontpage);

    app.handle_data(DataMsg::RepoFrontpage {
        repo: RepoId::new("owner", "repo"),
        description: "d".into(),
        readme: "r".into(),
    });

    assert_eq!(
        app.repo_ctx.repo_frontpage,
        Some(("d".to_string(), "r".to_string()))
    );
    assert!(app.loading.is_none());
}

#[test]
fn repo_frontpage_other_repo_not_applied() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.handle_data(DataMsg::RepoFrontpage {
        repo: RepoId::new("owner", "otherrepo"),
        description: "d".into(),
        readme: "r".into(),
    });

    assert!(app.repo_ctx.repo_frontpage.is_none());
}

#[tokio::test]
async fn issues_current_repo_selects_first() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.loading = Some(LoadingKind::Issues);

    app.handle_data(DataMsg::Issues {
        repo: RepoId::new("owner", "repo"),
        issues: vec![make_issue(1)],
        has_more: false,
    });

    assert_eq!(app.repo_ctx.issues.len(), 1);
    assert_eq!(app.repo_ctx.issue_state.selected(), Some(0));
    assert!(app.loading.is_none());
}

#[test]
fn issues_stale_repo_ignored() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.handle_data(DataMsg::Issues {
        repo: RepoId::new("owner", "otherrepo"),
        issues: vec![make_issue(1)],
        has_more: false,
    });

    assert!(app.repo_ctx.issues.is_empty());
}

#[test]
fn more_issues_extends() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.repo_ctx.issues = vec![make_issue(1)];

    app.handle_data(DataMsg::MoreIssues {
        repo: RepoId::new("owner", "repo"),
        issues: vec![make_issue(2)],
        has_more: false,
    });

    assert_eq!(app.repo_ctx.issues.len(), 2);
    assert!(!app.repo_ctx.issues_pagination.has_more);
    assert!(app.loading.is_none());
}

#[test]
fn more_issues_stale_repo_ignored() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.handle_data(DataMsg::MoreIssues {
        repo: RepoId::new("owner", "otherrepo"),
        issues: vec![make_issue(2)],
        has_more: false,
    });

    assert!(app.repo_ctx.issues.is_empty());
}

#[test]
fn issue_body_selected_issue_sets_body() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.repo_ctx.issues = vec![make_issue(5)];
    app.repo_ctx.issue_state.select(Some(0));

    app.handle_data(DataMsg::IssueBody {
        repo: RepoId::new("owner", "repo"),
        number: 5,
        body: "ib".into(),
    });

    assert_eq!(app.repo_ctx.issue_body, Some("ib".to_string()));
}

#[test]
fn issue_body_stale_repo_ignored() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.repo_ctx.issues = vec![make_issue(5)];
    app.repo_ctx.issue_state.select(Some(0));

    app.handle_data(DataMsg::IssueBody {
        repo: RepoId::new("owner", "otherrepo"),
        number: 5,
        body: "ib".into(),
    });

    assert!(app.repo_ctx.issue_body.is_none());
}

#[tokio::test]
async fn source_issues_accept_selects_and_caches() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.loading = Some(LoadingKind::Issues);

    app.handle_data(DataMsg::SourceIssues {
        owner: "owner".into(),
        issues: vec![make_issue(1)],
        has_more: true,
    });

    assert_eq!(app.source_ctx.source_issues.len(), 1);
    assert_eq!(app.source_ctx.source_issue_state.selected(), Some(0));
    assert_eq!(app.source_ctx.source_issues_pagination.page, 1);
    assert!(app.source_ctx.source_issues_pagination.has_more);
    assert!(app.source_issues_cache.contains_key("owner"));
    assert!(app.loading.is_none());
}

#[test]
fn source_issues_stale_owner_ignored() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.handle_data(DataMsg::SourceIssues {
        owner: "other".into(),
        issues: vec![make_issue(1)],
        has_more: false,
    });

    assert!(app.source_ctx.source_issues.is_empty());
    assert!(app.source_issues_cache.is_empty());
}

#[test]
fn more_source_issues_extends() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.source_ctx.source_issues = vec![make_issue(1)];

    app.handle_data(DataMsg::MoreSourceIssues {
        owner: "owner".into(),
        issues: vec![make_issue(2)],
        has_more: false,
    });

    assert_eq!(app.source_ctx.source_issues.len(), 2);
    assert!(!app.source_ctx.source_issues_pagination.has_more);
}

#[test]
fn more_source_issues_stale_owner_ignored() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.handle_data(DataMsg::MoreSourceIssues {
        owner: "other".into(),
        issues: vec![make_issue(2)],
        has_more: false,
    });

    assert!(app.source_ctx.source_issues.is_empty());
}

#[tokio::test]
async fn rate_limit_timestamp_updates_only_on_change() {
    let mut app = make_app();

    app.handle_data(DataMsg::RateLimit {
        remaining: 10,
        limit: 60,
    });
    assert_eq!(app.rate_limit, Some((10, 60)));
    assert!(app.rate_limit_updated_at.is_some());

    app.rate_limit_updated_at = None;
    app.handle_data(DataMsg::RateLimit {
        remaining: 10,
        limit: 60,
    });
    assert_eq!(app.rate_limit, Some((10, 60)));
    assert!(app.rate_limit_updated_at.is_none());

    app.handle_data(DataMsg::RateLimit {
        remaining: 9,
        limit: 60,
    });
    assert_eq!(app.rate_limit, Some((9, 60)));
    assert!(app.rate_limit_updated_at.is_some());
}

#[test]
fn viewer_permission_current_repo_sets_fields() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.handle_data(DataMsg::ViewerPermission {
        repo: RepoId::new("owner", "repo"),
        can_push: true,
        allow_auto_merge: false,
    });

    assert_eq!(app.repo_ctx.viewer_can_push, Some(true));
    assert_eq!(app.repo_ctx.allow_auto_merge, Some(false));
}

#[test]
fn viewer_permission_other_repo_not_applied() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.handle_data(DataMsg::ViewerPermission {
        repo: RepoId::new("owner", "otherrepo"),
        can_push: true,
        allow_auto_merge: false,
    });

    assert_eq!(app.repo_ctx.viewer_can_push, None);
    assert_eq!(app.repo_ctx.allow_auto_merge, None);
}

#[tokio::test]
async fn source_prs_accept_selects_and_caches() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.loading = Some(LoadingKind::Prs);

    app.handle_data(DataMsg::SourcePrs {
        owner: "owner".into(),
        prs: vec![make_pr_numbered(1)],
        has_more: true,
    });

    assert_eq!(app.source_ctx.source_prs.len(), 1);
    assert_eq!(app.source_ctx.source_pr_state.selected(), Some(0));
    assert_eq!(app.source_ctx.source_prs_pagination.page, 1);
    assert!(app.source_ctx.source_prs_pagination.has_more);
    assert!(app.source_prs_cache.contains_key("owner"));
    assert!(app.loading.is_none());
}

#[test]
fn source_prs_stale_owner_ignored() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.handle_data(DataMsg::SourcePrs {
        owner: "other".into(),
        prs: vec![make_pr_numbered(1)],
        has_more: false,
    });

    assert!(app.source_ctx.source_prs.is_empty());
    assert!(app.source_prs_cache.is_empty());
}

#[test]
fn more_source_prs_extends() {
    let mut app = make_app();
    setup_selected_repo(&mut app);
    app.source_ctx.source_prs = vec![make_pr_numbered(1)];

    app.handle_data(DataMsg::MoreSourcePrs {
        owner: "owner".into(),
        prs: vec![make_pr_numbered(2)],
        has_more: false,
    });

    assert_eq!(app.source_ctx.source_prs.len(), 2);
    assert!(!app.source_ctx.source_prs_pagination.has_more);
}

#[test]
fn more_source_prs_stale_owner_ignored() {
    let mut app = make_app();
    setup_selected_repo(&mut app);

    app.handle_data(DataMsg::MoreSourcePrs {
        owner: "other".into(),
        prs: vec![make_pr_numbered(2)],
        has_more: false,
    });

    assert!(app.source_ctx.source_prs.is_empty());
}

#[test]
fn action_done_some_sets_status_and_clears_loading() {
    let mut app = make_app();
    app.loading = Some(LoadingKind::Action("x".into()));

    app.handle_data(DataMsg::ActionDone(Some("done!".into())));

    assert_eq!(app.status_msg.as_ref().unwrap().0, "done!");
    assert!(app.loading.is_none());
}

#[test]
fn action_done_none_clears_loading_only() {
    let mut app = make_app();
    app.loading = Some(LoadingKind::Action("x".into()));

    app.handle_data(DataMsg::ActionDone(None));

    assert!(app.status_msg.is_none());
    assert!(app.loading.is_none());
}

#[test]
fn error_sets_status_prefixed() {
    let mut app = make_app();
    app.loading = Some(LoadingKind::Prs);

    app.handle_data(DataMsg::Error("boom".into()));

    assert_eq!(app.status_msg.as_ref().unwrap().0, "Error: boom");
    assert!(app.loading.is_none());
}
