use ghview::app::App;
use ghview::types::Source;
use serde_json;

use super::builders;

pub fn app_with_repo_list() -> App {
    let mut app = builders::make_app();
    app.sources = vec![Source::User("owner".into())];
    app.source_state.select(Some(0));

    let path = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/repos_org.jsonl"
    ));
    let repos = if path.exists() {
        let content = std::fs::read_to_string(path).expect("read repos fixture");
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<ghview::types::Repo>(l).ok())
            .collect()
    } else {
        vec![
            builders::make_repo("alpha"),
            builders::make_repo("beta"),
            builders::make_repo("gamma"),
        ]
    };

    app.source_ctx.repos = repos;
    app.source_ctx.repo_state.select(Some(0));
    app
}

pub fn app_with_prs() -> App {
    let mut app = builders::make_app();
    builders::setup_selected_repo(&mut app);
    app.repo_ctx.prs_raw = vec![builders::make_pr_numbered(1), builders::make_pr_numbered(2)];
    app.repo_ctx.prs = app.repo_ctx.prs_raw.clone();
    app.repo_ctx.pr_state.select(Some(0));
    app
}

pub fn app_with_pr_detail() -> App {
    let mut app = app_with_prs();
    app.repo_ctx.pr_body = Some("PR body".into());
    app
}

pub fn app_with_issues() -> App {
    let mut app = builders::make_app();
    builders::setup_selected_repo(&mut app);
    app.repo_ctx.issues = vec![builders::make_issue(1), builders::make_issue(2)];
    app.repo_ctx.issue_state.select(Some(0));
    app
}

pub fn app_with_source_prs() -> App {
    let mut app = builders::make_app();
    app.sources = vec![Source::User("owner".into())];
    app.source_state.select(Some(0));
    app.source_ctx.source_prs = vec![builders::make_pr_numbered(1)];
    app.source_ctx.source_pr_state.select(Some(0));
    app
}

pub fn app_with_source_issues() -> App {
    let mut app = builders::make_app();
    app.sources = vec![Source::User("owner".into())];
    app.source_state.select(Some(0));
    app.source_ctx.source_issues = vec![builders::make_issue(1)];
    app.source_ctx.source_issue_state.select(Some(0));
    app
}

pub fn app_with_frontpage() -> App {
    let mut app = builders::make_app();
    builders::setup_selected_repo(&mut app);
    app.repo_ctx.repo_frontpage = Some(("README.md".into(), "# hello".into()));
    app
}
