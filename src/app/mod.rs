mod event_loop;
mod handlers;
mod nav;
mod triggers;

pub use event_loop::{InteractiveCmd, InteractiveKind, run_event_loop};

#[derive(Debug, Default)]
pub struct PaginationState {
    pub page: u32,
    pub has_more: bool,
    pub fetching_more: bool,
}

impl PaginationState {
    pub fn can_load_more(&self) -> bool {
        !self.fetching_more && self.has_more
    }

    pub fn begin_fetch(&mut self) -> u32 {
        self.page += 1;
        self.fetching_more = true;
        self.page
    }

    pub fn reset(&mut self, has_more: bool) {
        self.page = 1;
        self.has_more = has_more;
        self.fetching_more = false;
    }

    pub fn finish(&mut self, has_more: bool) {
        self.has_more = has_more;
        self.fetching_more = false;
    }
}

use crate::{
    config::Config,
    keys::Action,
    types::{
        CheckRun, CheckStatus, Column, DataMsg, DetailSection, DiffView, Issue, LoadingKind,
        MergeableState, PR, PrAction, PrId, Repo, RepoId, RepoSortKey, RepoView, ReposView,
        ReviewStatus, SortKey, Source,
    },
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::ListState;
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;

pub const DEPENDABOT_COMMANDS: &[(char, &str)] = &[
    ('r', "rebase"),
    ('e', "recreate"),
    ('m', "merge"),
    ('s', "squash and merge"),
    ('c', "cancel merge"),
    ('x', "close"),
    ('o', "reopen"),
    ('i', "ignore this dependency"),
    ('I', "ignore this major version"),
    ('j', "ignore this minor version"),
    ('k', "ignore this patch version"),
    ('u', "unignore *"),
];

#[derive(Debug, Default)]
pub struct RepoCtx {
    pub prs_raw: Vec<PR>,
    pub prs: Vec<PR>,
    pub pr_state: ListState,
    pub prs_pagination: PaginationState,
    pub pr_body: Option<String>,
    pub check_runs: Option<Vec<CheckRun>>,
    pub check_runs_state: ListState,
    pub pr_body_scroll: u16,
    pub detail_section: DetailSection,
    pub diff_view: Option<DiffView>,
    pub review_statuses: HashMap<u64, ReviewStatus>,
    pub mergeable_states: HashMap<PrId, MergeableState>,
    pub check_summary_cache: HashMap<PrId, CheckStatus>,
    pub issues: Vec<Issue>,
    pub issue_state: ListState,
    pub issues_pagination: PaginationState,
    pub issue_body: Option<String>,
    pub issue_body_scroll: u16,
    pub repo_frontpage: Option<(String, String)>,
    pub repo_frontpage_scroll: u16,
    pub viewer_can_push: Option<bool>,
    pub allow_auto_merge: Option<bool>,
}

#[derive(Debug, Default)]
pub struct SourceCtx {
    pub repos: Vec<Repo>,
    pub repo_state: ListState,
    pub repo_filter: String,
    pub repos_pagination: PaginationState,
    pub source_prs: Vec<PR>,
    pub source_pr_state: ListState,
    pub source_pr_filter: String,
    pub source_prs_pagination: PaginationState,
    pub source_issues: Vec<Issue>,
    pub source_issue_state: ListState,
    pub source_issue_filter: String,
    pub source_issues_pagination: PaginationState,
}

pub struct App {
    pub focus: Column,

    pub sources: Vec<Source>,
    pub source_state: ListState,
    pub source_filter: String,
    pub current_user: Option<String>,

    pub repo_ctx: RepoCtx,
    pub source_ctx: SourceCtx,

    pub repo_cache: HashMap<(String, RepoSortKey), (Instant, Vec<Repo>)>,
    pub pr_filter: String,
    pub pr_cache: HashMap<String, (Instant, Vec<PR>)>,
    pub(crate) frontpage_cache: HashMap<String, (Instant, (String, String))>,
    pub(crate) permission_cache: HashMap<String, (Instant, (bool, bool))>,

    pub(crate) review_cache: HashMap<String, HashMap<u64, ReviewStatus>>,

    pub filter_active: bool,
    pub sort_key: SortKey,
    pub repo_sort_key: RepoSortKey,

    pub rate_limit: Option<(u32, u32)>,
    pub rate_limit_updated_at: Option<Instant>,

    pub loading: Option<LoadingKind>,
    pub config: Config,
    pub status_msg: Option<(String, bool)>,
    pub(crate) status_msg_at: Option<Instant>,
    pub show_help: bool,
    pub help_scroll: u16,
    pub show_dependabot_menu: bool,
    pub repo_view: RepoView,
    pub repos_view: ReposView,

    pub source_prs_cache: HashMap<String, (Instant, Vec<PR>)>,
    pub source_issues_cache: HashMap<String, (Instant, Vec<Issue>)>,

    pub terminal_height: u16,
    pub should_quit: bool,
    pub now_override: Option<jiff::Timestamp>,
    pub(crate) tx: UnboundedSender<DataMsg>,
}

fn filter_visible<'a, T>(
    items: &'a [T],
    filter: &str,
    matches: impl Fn(&T, &str) -> bool,
) -> Vec<&'a T> {
    if filter.is_empty() {
        items.iter().collect()
    } else {
        let f = filter.to_lowercase();
        items.iter().filter(|item| matches(item, &f)).collect()
    }
}

impl App {
    pub fn new(tx: UnboundedSender<DataMsg>, config: Config) -> Self {
        Self {
            focus: Column::Sources,
            sources: vec![],
            source_state: ListState::default(),
            source_filter: String::new(),
            current_user: None,
            repo_ctx: RepoCtx::default(),
            source_ctx: SourceCtx::default(),
            pr_filter: String::new(),
            pr_cache: HashMap::new(),
            review_cache: HashMap::new(),
            filter_active: false,
            sort_key: SortKey::Newest,
            repo_sort_key: config.ui.repo_sort,
            repo_view: config.ui.default_repo_view,
            repos_view: config.ui.default_repos_view,
            rate_limit: None,
            rate_limit_updated_at: None,
            loading: None,
            config,
            status_msg: None,
            status_msg_at: None,
            show_help: false,
            help_scroll: 0,
            show_dependabot_menu: false,
            repo_cache: HashMap::new(),
            source_prs_cache: HashMap::new(),
            source_issues_cache: HashMap::new(),
            frontpage_cache: HashMap::new(),
            permission_cache: HashMap::new(),
            terminal_height: 40,
            should_quit: false,
            now_override: None,
            tx,
        }
    }

    /// Current time, overridable by tests for deterministic rendering.
    pub fn now(&self) -> jiff::Timestamp {
        self.now_override.unwrap_or_else(jiff::Timestamp::now)
    }

    pub fn resume(mut self, tx: UnboundedSender<DataMsg>) -> Self {
        self.tx = tx;
        self.loading = None;
        self.status_msg = None;
        self.show_help = false;
        self.help_scroll = 0;
        self.show_dependabot_menu = false;
        self.repo_ctx.diff_view = None;
        self.should_quit = false;
        self
    }

    pub fn visible_sources(&self) -> Vec<&Source> {
        filter_visible(&self.sources, &self.source_filter, |s, f| {
            s.owner().to_lowercase().contains(f)
        })
    }

    pub fn visible_repos(&self) -> Vec<&Repo> {
        filter_visible(
            &self.source_ctx.repos,
            &self.source_ctx.repo_filter,
            |r, f| r.name.to_lowercase().contains(f),
        )
    }

    pub fn visible_source_prs(&self) -> Vec<&PR> {
        filter_visible(
            &self.source_ctx.source_prs,
            &self.source_ctx.source_pr_filter,
            |pr, f| {
                pr.title.to_lowercase().contains(f)
                    || pr.author.to_lowercase().contains(f)
                    || pr.repo.to_lowercase().contains(f)
            },
        )
    }

    pub fn visible_source_issues(&self) -> Vec<&Issue> {
        filter_visible(
            &self.source_ctx.source_issues,
            &self.source_ctx.source_issue_filter,
            |issue, f| {
                issue.title.to_lowercase().contains(f)
                    || issue.author.to_lowercase().contains(f)
                    || issue.repo.to_lowercase().contains(f)
            },
        )
    }

    pub fn selected_source(&self) -> Option<&Source> {
        let vs = self.visible_sources();
        self.source_state
            .selected()
            .and_then(|i| vs.get(i).copied())
    }

    pub fn selected_source_owner(&self) -> Option<String> {
        self.selected_source().map(|s| s.owner().to_string())
    }

    pub fn selected_repo(&self) -> Option<&str> {
        let vr = self.visible_repos();
        self.source_ctx
            .repo_state
            .selected()
            .and_then(|i| vr.get(i).map(|r| r.name.as_str()))
    }

    pub fn merge_uses_auto(&self) -> bool {
        if !self.config.ui.merge_auto {
            return false;
        }
        if self.repos_view != ReposView::PrList {
            // Per-repo view: use allow_auto_merge fetched from the individual repo
            // endpoint alongside viewer permissions. Defaults false until that arrives.
            return self.repo_ctx.allow_auto_merge.unwrap_or(false);
        }
        // Source PR list: look up the PR's repo in the already-loaded repos list.
        let Some(repo_name) = self.selected_pr().map(|pr| pr.repo.as_str()) else {
            return false;
        };
        self.source_ctx
            .repos
            .iter()
            .find(|r| r.name == repo_name)
            .is_some_and(|r| r.allow_auto_merge)
    }

    pub fn selected_repo_has_issues(&self) -> bool {
        let vr = self.visible_repos();
        self.source_ctx
            .repo_state
            .selected()
            .and_then(|i| vr.get(i))
            .is_none_or(|r| r.has_issues)
    }

    pub fn selected_repo_has_prs(&self) -> bool {
        let vr = self.visible_repos();
        self.source_ctx
            .repo_state
            .selected()
            .and_then(|i| vr.get(i))
            .is_none_or(|r| r.has_pull_requests)
    }

    pub fn selected_pr(&self) -> Option<&PR> {
        if self.repos_view == ReposView::PrList {
            let visible = self.visible_source_prs();
            return self
                .source_ctx
                .source_pr_state
                .selected()
                .and_then(|i| visible.get(i).copied());
        }
        self.repo_ctx
            .pr_state
            .selected()
            .and_then(|i| self.repo_ctx.prs.get(i))
    }

    pub(crate) fn selected_pr_context(&self) -> Option<(RepoId, PR)> {
        let rid = self.selected_owner_repo()?;
        let pr = self.selected_pr()?.clone();
        Some((rid, pr))
    }

    pub(crate) fn selected_pr_id(&self) -> Option<PrId> {
        let rid = self.selected_owner_repo()?;
        let number = self.selected_pr()?.number;
        Some(rid.pr(number))
    }

    pub(crate) fn selected_issue_context(&self) -> Option<(RepoId, Issue)> {
        let rid = self.selected_owner_repo()?;
        let issue = self.selected_issue()?.clone();
        Some((rid, issue))
    }

    pub fn selected_issue(&self) -> Option<&Issue> {
        if self.repos_view == ReposView::IssueList {
            let visible = self.visible_source_issues();
            return self
                .source_ctx
                .source_issue_state
                .selected()
                .and_then(|i| visible.get(i).copied());
        }
        self.repo_ctx
            .issue_state
            .selected()
            .and_then(|i| self.repo_ctx.issues.get(i))
    }

    pub fn pr_body_focusable(&self) -> bool {
        self.repo_ctx.pr_body.as_deref() != Some("")
    }

    pub fn checks_focusable(&self) -> bool {
        self.repo_ctx
            .check_runs
            .as_ref()
            .is_none_or(|runs| !runs.is_empty())
    }

    pub fn action_permitted(&self, action: Action) -> bool {
        let pr = self.selected_pr();
        let current_user = self.current_user.as_deref().unwrap_or("");
        let is_author = pr.is_some_and(|p| p.author == current_user);
        let can_push = self.repo_ctx.viewer_can_push.unwrap_or(true);
        match action {
            Action::Approve => !is_author,
            Action::Merge | Action::CheckRerun | Action::DependabotMenu => can_push,
            Action::ClosePr | Action::ReopenPr | Action::MarkReady => can_push || is_author,
            _ => true,
        }
    }

    pub fn selected_pr_is_dependabot(&self) -> bool {
        self.selected_pr()
            .is_some_and(|pr| matches!(pr.author.as_str(), "dependabot[bot]" | "dependabot"))
    }

    pub fn set_status(&mut self, msg: String) {
        self.status_msg = Some((msg, false));
        self.status_msg_at = Some(Instant::now());
    }

    pub fn set_error(&mut self, msg: String) {
        self.status_msg = Some((msg, true));
        self.status_msg_at = Some(Instant::now());
    }

    pub fn clear_status_if_expired(&mut self) {
        if let Some(at) = self.status_msg_at
            && at.elapsed() > std::time::Duration::from_secs(4)
        {
            self.status_msg = None;
            self.status_msg_at = None;
        }
    }

    pub fn handle_filter_input(&mut self, key: KeyEvent) {
        let prev_source = self.selected_source_owner();
        let prev_repo = self.selected_repo().map(str::to_string);
        let prev_source_pr_num = (self.repos_view == ReposView::PrList)
            .then(|| self.selected_pr().map(|p| p.number))
            .flatten();
        let prev_source_issue_num = (self.repos_view == ReposView::IssueList)
            .then(|| self.selected_source_issue().map(|i| i.number))
            .flatten();

        match key.code {
            KeyCode::Esc => {
                *self.active_filter_mut() = String::new();
                self.filter_active = false;
                self.clamp_source_selection();
                self.clamp_repo_selection();
                self.clamp_source_pr_selection();
                self.clamp_source_issue_selection();
                self.rebuild_prs();
            }
            KeyCode::Enter => {
                self.filter_active = false;
            }
            KeyCode::Backspace => {
                self.active_filter_mut().pop();
                self.clamp_source_selection();
                self.clamp_repo_selection();
                self.clamp_source_pr_selection();
                self.clamp_source_issue_selection();
                self.rebuild_prs();
            }
            KeyCode::Char(c) => {
                self.active_filter_mut().push(c);
                self.clamp_source_selection();
                self.clamp_repo_selection();
                self.clamp_source_pr_selection();
                self.clamp_source_issue_selection();
                self.rebuild_prs();
            }
            _ => {}
        }

        if self.selected_source_owner() != prev_source {
            self.on_source_changed();
        } else if self.selected_repo().map(str::to_string) != prev_repo {
            self.on_repo_changed();
        } else if self.repos_view == ReposView::PrList
            && self.selected_pr().map(|p| p.number) != prev_source_pr_num
        {
            self.trigger_load_pr_body();
        } else if self.repos_view == ReposView::IssueList
            && self.selected_source_issue().map(|i| i.number) != prev_source_issue_num
        {
            self.trigger_load_source_issue_body();
        }
    }

    fn active_filter_mut(&mut self) -> &mut String {
        match self.focus {
            Column::Sources | Column::Detail => &mut self.source_filter,
            Column::Repos => match self.repos_view {
                ReposView::PrList => &mut self.source_ctx.source_pr_filter,
                ReposView::IssueList => &mut self.source_ctx.source_issue_filter,
                ReposView::RepoList => &mut self.source_ctx.repo_filter,
            },
            Column::Repo => &mut self.pr_filter,
        }
    }

    pub fn active_filter(&self) -> &str {
        match self.focus {
            Column::Sources | Column::Detail => &self.source_filter,
            Column::Repos => match self.repos_view {
                ReposView::PrList => &self.source_ctx.source_pr_filter,
                ReposView::IssueList => &self.source_ctx.source_issue_filter,
                ReposView::RepoList => &self.source_ctx.repo_filter,
            },
            Column::Repo => &self.pr_filter,
        }
    }

    pub fn handle_action(&mut self, action: Action) {
        if self.repo_ctx.diff_view.is_some() {
            match action {
                Action::Quit | Action::Left => self.repo_ctx.diff_view = None,
                Action::Down | Action::Right => self.diff_scroll(3),
                Action::Up => self.diff_scroll_up(3),
                Action::Bottom => {
                    if let Some(d) = &mut self.repo_ctx.diff_view {
                        d.scroll =
                            u16::try_from(d.lines.len().saturating_sub(1)).unwrap_or(u16::MAX);
                    }
                }
                Action::Top => {
                    if let Some(d) = &mut self.repo_ctx.diff_view {
                        d.scroll = 0;
                    }
                }
                _ => {}
            }
            return;
        }

        if self.show_help {
            self.show_help = false;
            return;
        }
        if self.show_dependabot_menu {
            self.show_dependabot_menu = false;
            return;
        }

        match action {
            Action::Quit => self.should_quit = true,
            Action::Help => {
                self.show_help = true;
                self.help_scroll = 0;
            }
            Action::Refresh => self.trigger_refresh(),
            Action::FilterStart => self.filter_active = true,
            Action::SortCycle => {
                if self.focus == Column::Repos && self.repos_view == ReposView::RepoList {
                    self.repo_sort_key = self.repo_sort_key.next();
                    self.force_load_repos();
                } else if !matches!(self.repos_view, ReposView::PrList | ReposView::IssueList) {
                    self.sort_key = self.sort_key.next();
                    self.repo_ctx.pr_state.select(Some(0));
                    self.rebuild_prs();
                }
            }

            Action::Up => self.move_up(),
            Action::Down => self.move_down(),
            Action::Left => self.move_left(),
            Action::Right => self.move_right(),
            Action::Top => self.move_top(),
            Action::Bottom => self.move_bottom(),

            Action::ViewRepos => {
                if self.focus == Column::Repos {
                    self.repos_view = ReposView::RepoList;
                }
            }
            Action::ViewPrs => {
                if self.focus == Column::Repos {
                    self.repos_view = ReposView::PrList;
                    if self.source_ctx.source_prs.is_empty() {
                        self.trigger_load_source_prs();
                    } else {
                        self.trigger_load_pr_body();
                    }
                }
            }
            Action::ViewIssues => {
                if self.focus == Column::Repos {
                    self.repos_view = ReposView::IssueList;
                    if self.source_ctx.source_issues.is_empty() {
                        self.trigger_load_source_issues();
                    } else {
                        self.trigger_load_source_issue_body();
                    }
                }
            }

            Action::OpenBrowser => self.context_open_browser(),
            Action::OpenIssues => self.context_open_issues(),
            Action::CopyUrl => self.context_copy_url(),

            Action::Approve => self.do_pr_action(PrAction::Approve),
            Action::Merge => self.do_pr_action(PrAction::Merge),
            Action::ClosePr => self.do_pr_action(PrAction::Close),
            Action::ReopenPr => self.do_pr_action(PrAction::Reopen),
            Action::MarkReady => self.do_pr_action(PrAction::MarkReady),
            Action::DependabotMenu => {
                if self.selected_pr_is_dependabot() {
                    self.show_dependabot_menu = true;
                } else {
                    self.set_error("Not a Dependabot PR".to_string());
                }
            }
            Action::Diff => self.trigger_load_diff(),

            Action::CheckOpen => self.open_selected_check(),
            Action::CheckRerun => self.rerun_selected_check(),

            Action::Checkout | Action::Comment => {}
        }
    }

    pub fn handle_dependabot_key(&mut self, key: char) -> bool {
        self.show_dependabot_menu = false;
        let Some((_, cmd)) = DEPENDABOT_COMMANDS.iter().find(|(k, _)| *k == key) else {
            return true;
        };
        self.post_dependabot_comment(&format!("@dependabot {cmd}"));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config,
        types::{PR, PrState, Repo},
    };

    fn make_app() -> App {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        App::new(tx, Config::default())
    }

    fn make_pr(author: &str) -> PR {
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

    #[test]
    fn dependabot_bot_is_recognized() {
        let mut app = make_app();
        app.repo_ctx.prs = vec![make_pr("dependabot[bot]")];
        app.repo_ctx.pr_state.select(Some(0));
        assert!(app.selected_pr_is_dependabot());
    }

    #[test]
    fn dependabot_legacy_name_recognized() {
        let mut app = make_app();
        app.repo_ctx.prs = vec![make_pr("dependabot")];
        app.repo_ctx.pr_state.select(Some(0));
        assert!(app.selected_pr_is_dependabot());
    }

    #[test]
    fn dependabot_prefix_only_not_recognized() {
        let mut app = make_app();
        app.repo_ctx.prs = vec![make_pr("dependabot-hacker")];
        app.repo_ctx.pr_state.select(Some(0));
        assert!(!app.selected_pr_is_dependabot());
    }

    #[test]
    fn regular_user_not_dependabot() {
        let mut app = make_app();
        app.repo_ctx.prs = vec![make_pr("alice")];
        app.repo_ctx.pr_state.select(Some(0));
        assert!(!app.selected_pr_is_dependabot());
    }

    #[test]
    fn no_selected_pr_not_dependabot() {
        let app = make_app();
        assert!(!app.selected_pr_is_dependabot());
    }

    #[test]
    fn visible_sources_no_filter_returns_all() {
        let mut app = make_app();
        app.sources = vec![Source::User("alice".into()), Source::Org("my-org".into())];
        assert_eq!(app.visible_sources().len(), 2);
    }

    #[test]
    fn visible_sources_filter_case_insensitive() {
        let mut app = make_app();
        app.sources = vec![Source::User("alice".into()), Source::Org("my-org".into())];
        app.source_filter = "ALI".into();
        let visible = app.visible_sources();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].owner(), "alice");
    }

    #[test]
    fn visible_repos_filter_by_name() {
        let mut app = make_app();
        app.source_ctx.repos = vec![
            Repo {
                name: "frontend".into(),
                ..Repo::default()
            },
            Repo {
                name: "backend".into(),
                ..Repo::default()
            },
        ];
        app.source_ctx.repo_filter = "front".into();
        let visible = app.visible_repos();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].name, "frontend");
    }

    #[test]
    fn rebuild_prs_filter_by_title() {
        let mut app = make_app();
        let mut p1 = make_pr("alice");
        p1.title = "Add feature".into();
        let mut p2 = make_pr("bob");
        p2.title = "Fix login bug".into();
        app.repo_ctx.prs_raw = vec![p1, p2];
        app.pr_filter = "login".into();
        app.rebuild_prs();
        assert_eq!(app.repo_ctx.prs.len(), 1);
        assert_eq!(app.repo_ctx.prs[0].author, "bob");
    }

    #[test]
    fn rebuild_prs_filter_by_author() {
        let mut app = make_app();
        app.repo_ctx.prs_raw = vec![make_pr("alice"), make_pr("bob")];
        app.pr_filter = "bob".into();
        app.rebuild_prs();
        assert_eq!(app.repo_ctx.prs.len(), 1);
        assert_eq!(app.repo_ctx.prs[0].author, "bob");
    }

    #[test]
    fn rebuild_prs_empty_filter_keeps_all() {
        let mut app = make_app();
        app.repo_ctx.prs_raw = vec![make_pr("alice"), make_pr("bob")];
        app.rebuild_prs();
        assert_eq!(app.repo_ctx.prs.len(), 2);
    }

    fn setup_selected_repo(app: &mut App) {
        app.sources = vec![Source::User("owner".into())];
        app.source_state.select(Some(0));
        app.source_ctx.repos = vec![Repo {
            name: "repo".into(),
            ..Repo::default()
        }];
        app.source_ctx.repo_state.select(Some(0));
    }

    fn make_pr_numbered(number: u64) -> PR {
        PR {
            number,
            title: "test pr".into(),
            author: "alice".into(),
            draft: false,
            state: PrState::Open,
            created_at: "2024-01-01T00:00:00Z".into(),
            updated_at: "2024-01-01T00:00:00Z".into(),
            url: "https://github.com/owner/repo/pull/1".into(),
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

    #[test]
    fn diff_content_accepted_for_current_pr() {
        use crate::types::{DataMsg, RepoId};
        let mut app = make_app();
        setup_selected_repo(&mut app);
        app.repo_ctx.prs = vec![make_pr_numbered(42)];
        app.repo_ctx.pr_state.select(Some(0));
        app.handle_data(DataMsg::DiffContent {
            pr: RepoId::new("owner", "repo").pr(42),
            title: "t".into(),
            content: "diff\n".into(),
        });
        assert!(app.repo_ctx.diff_view.is_some());
    }

    #[test]
    fn diff_content_ignored_for_wrong_repo() {
        use crate::types::{DataMsg, RepoId};
        let mut app = make_app();
        setup_selected_repo(&mut app);
        app.repo_ctx.prs = vec![make_pr_numbered(42)];
        app.repo_ctx.pr_state.select(Some(0));
        app.handle_data(DataMsg::DiffContent {
            pr: RepoId::new("other", "repo").pr(42),
            title: "t".into(),
            content: "diff\n".into(),
        });
        assert!(app.repo_ctx.diff_view.is_none());
    }

    #[test]
    fn diff_content_ignored_for_wrong_pr_number() {
        use crate::types::{DataMsg, RepoId};
        let mut app = make_app();
        setup_selected_repo(&mut app);
        app.repo_ctx.prs = vec![make_pr_numbered(42)];
        app.repo_ctx.pr_state.select(Some(0));
        app.handle_data(DataMsg::DiffContent {
            pr: RepoId::new("owner", "repo").pr(99),
            title: "t".into(),
            content: "diff\n".into(),
        });
        assert!(app.repo_ctx.diff_view.is_none());
    }

    #[test]
    fn diff_content_splits_into_lines() {
        use crate::types::{DataMsg, RepoId};
        let mut app = make_app();
        setup_selected_repo(&mut app);
        app.repo_ctx.prs = vec![make_pr_numbered(42)];
        app.repo_ctx.pr_state.select(Some(0));
        app.handle_data(DataMsg::DiffContent {
            pr: RepoId::new("owner", "repo").pr(42),
            title: "t".into(),
            content: "line1\nline2\n".into(),
        });
        let diff = app.repo_ctx.diff_view.as_ref().expect("diff_view is None");
        assert_eq!(diff.lines.len(), 2);
    }
}
