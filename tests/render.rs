mod common;
use common::{builders, inflate};
use ghview::types::{Column, DiffView, RepoView, ReposView};
use ratatui::{Terminal, backend::TestBackend};

fn render(name: &str, app: &mut ghview::app::App, width: u16, height: u16) {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ghview::ui::draw(f, app)).unwrap();
    insta::assert_snapshot!(name, terminal.backend());
}

#[tokio::test]
async fn preview_repo_list() {
    let mut app = inflate::app_with_repo_list().await;
    render("preview_repo_list", &mut app, 120, 40);
}

#[tokio::test]
async fn preview_repo_list_direct_source() {
    let mut app = inflate::app_with_repo_list().await;
    app.direct_source = true;
    render("preview_repo_list_direct_source", &mut app, 120, 40);
}

#[tokio::test]
async fn preview_pr_list() {
    let mut app = inflate::app_with_source_prs().await;
    app.repos_view = ReposView::PrList;
    render("preview_pr_list", &mut app, 120, 40);
}

#[tokio::test]
async fn preview_issue_list() {
    let mut app = inflate::app_with_source_issues().await;
    app.repos_view = ReposView::IssueList;
    render("preview_issue_list", &mut app, 120, 40);
}

#[tokio::test]
async fn detail_pr_list() {
    let mut app = inflate::app_with_source_prs().await;
    app.repos_view = ReposView::PrList;
    app.focus = Column::Repo;
    render("detail_pr_list", &mut app, 120, 40);
}

#[tokio::test]
async fn detail_issue_list() {
    let mut app = inflate::app_with_source_issues().await;
    app.repos_view = ReposView::IssueList;
    app.focus = Column::Repo;
    render("detail_issue_list", &mut app, 120, 40);
}

#[tokio::test]
async fn detail_frontpage() {
    let mut app = inflate::app_with_frontpage().await;
    app.focus = Column::Repo;
    app.repo_view = RepoView::Frontpage;
    render("detail_frontpage", &mut app, 120, 40);
}

#[tokio::test]
async fn detail_prs() {
    let mut app = inflate::app_with_prs().await;
    app.focus = Column::Repo;
    render("detail_prs", &mut app, 120, 40);
}

#[tokio::test]
async fn detail_issues() {
    let mut app = inflate::app_with_issues().await;
    app.focus = Column::Repo;
    app.repo_view = RepoView::Issues;
    render("detail_issues", &mut app, 120, 40);
}

#[tokio::test]
async fn direct_prs() {
    let mut app = inflate::app_with_prs().await;
    app.direct_repo = true;
    app.focus = Column::Repo;
    render("direct_prs", &mut app, 120, 40);
}

#[tokio::test]
async fn direct_frontpage() {
    let mut app = inflate::app_with_frontpage().await;
    app.direct_repo = true;
    app.focus = Column::Repo;
    app.repo_view = RepoView::Frontpage;
    render("direct_frontpage", &mut app, 120, 40);
}

#[tokio::test]
async fn direct_issues() {
    let mut app = inflate::app_with_issues().await;
    app.direct_repo = true;
    app.focus = Column::Repo;
    app.repo_view = RepoView::Issues;
    render("direct_issues", &mut app, 120, 40);
}

#[tokio::test]
async fn help_overlay_shown() {
    let mut app = inflate::app_with_prs().await;
    app.focus = Column::Repo;
    app.show_help = true;
    render("help_overlay_shown", &mut app, 120, 40);
}

#[tokio::test]
async fn help_overlay_shown_direct_repo() {
    let mut app = inflate::app_with_prs().await;
    app.direct_repo = true;
    app.direct_source = true;
    app.focus = Column::Repo;
    app.show_help = true;
    render("help_overlay_shown_direct_repo", &mut app, 120, 40);
}

#[tokio::test]
async fn dependabot_menu_overlay_shown() {
    let mut app = inflate::app_with_prs().await;
    app.focus = Column::Repo;
    app.show_dependabot_menu = true;
    render("dependabot_menu_overlay_shown", &mut app, 120, 40);
}

#[tokio::test]
async fn diff_view_overlay_shown() {
    let mut app = inflate::app_with_prs().await;
    app.focus = Column::Repo;
    app.repo_ctx.diff_view = Some(DiffView {
        title: "src/main.rs".into(),
        lines: vec![
            "diff --git a/src/main.rs b/src/main.rs".to_string(),
            "--- a/src/main.rs".to_string(),
            "+++ b/src/main.rs".to_string(),
            "@@ -1,3 +1,4 @@".to_string(),
            " fn main() {".to_string(),
            "+    println!(\"added\");".to_string(),
            "-    println!(\"removed\");".to_string(),
            " }".to_string(),
        ]
        .into_boxed_slice(),
        scroll: 0,
    });
    render("diff_view_overlay_shown", &mut app, 120, 40);
}

#[test]
fn empty_app() {
    let mut app = builders::make_app();
    render("empty_app", &mut app, 120, 40);
}

#[test]
fn status_message_error() {
    let mut app = builders::make_app();
    app.status_msg = Some(("Something went wrong".to_string(), true));
    render("status_message_error", &mut app, 120, 40);
}

#[tokio::test]
async fn narrow_60x20() {
    let mut app = inflate::app_with_prs().await;
    app.focus = Column::Repo;
    render("narrow_60x20", &mut app, 60, 20);
}

#[tokio::test]
async fn medium_80x24() {
    let mut app = inflate::app_with_repo_list().await;
    render("medium_80x24", &mut app, 80, 24);
}

#[tokio::test]
async fn wide_200x50() {
    let mut app = inflate::app_with_prs().await;
    app.focus = Column::Repo;
    render("wide_200x50", &mut app, 200, 50);
}
