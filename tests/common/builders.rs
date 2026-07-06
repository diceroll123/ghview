use ghview::app::App;
use ghview::config::Config;
use ghview::types::{Issue, PR, PrState, Repo, Source};

pub fn make_app() -> App {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(tx, Config::default());
    app.now_override = Some(crate::common::fixed_now());
    app.loading = None;
    app.rate_limit_updated_at = None;
    app
}

pub fn make_pr(author: &str) -> PR {
    PR {
        number: 1,
        title: "test pr".into(),
        author: author.into(),
        draft: false,
        state: PrState::Open,
        created_at: "2024-01-01T00:00:00Z".into(),
        updated_at: "2024-01-01T00:00:00Z".into(),
        url: "https://github.com/o/r/pull/1".into(),
        requested_reviewers: vec![],
        labels: vec![],
        head_ref: "branch".into(),
        base_ref: "main".into(),
        head_sha: "abc".into(),
        additions: 0,
        deletions: 0,
        comments: 0,
        auto_merge: false,
        viewer_approved: false,
        repo: String::new(),
        repo_owner: String::new(),
    }
}

pub fn make_pr_numbered(number: u64) -> PR {
    let mut pr = make_pr("alice");
    pr.number = number;
    pr.url = "https://github.com/owner/repo/pull/1".into();
    pr
}

pub fn setup_selected_repo(app: &mut App) {
    app.sources = vec![Source::User("owner".into())];
    app.source_state.select(Some(0));
    app.source_ctx.repos = vec![Repo {
        name: "repo".into(),
        ..Repo::default()
    }];
    app.source_ctx.repo_state.select(Some(0));
}

pub fn make_repo(name: &str) -> Repo {
    Repo {
        name: name.into(),
        ..Repo::default()
    }
}

pub fn make_issue(number: u64) -> Issue {
    Issue {
        number,
        title: "test issue".into(),
        author: "alice".into(),
        created_at: "2024-01-01T00:00:00Z".into(),
        labels: vec![],
        url: "https://github.com/owner/repo/issues/1".into(),
        repo: String::new(),
        repo_owner: String::new(),
    }
}
