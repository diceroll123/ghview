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
        MergeableState, PR, PrAction, PrComment, PrCommit, PrFile, PrId, Repo, RepoId, RepoSortKey,
        RepoView, ReposView, ReviewStatus, SortKey, Source,
    },
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::ListState;
use std::collections::{HashMap, HashSet};
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
    pub pr_activity: Option<Vec<PrComment>>,
    pub pr_activity_scroll: u16,
    pub pr_commits: Option<Vec<PrCommit>>,
    pub pr_commits_state: ListState,
    pub pr_files: Option<Vec<PrFile>>,
    pub pr_files_state: ListState,
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
    pub direct_repo: bool,
    pub direct_source: bool,

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
    pub pending_clone_org: Option<String>,
    pub repo_view: RepoView,
    pub repos_view: ReposView,

    pub source_prs_cache: HashMap<String, (Instant, Vec<PR>)>,
    pub source_issues_cache: HashMap<String, (Instant, Vec<Issue>)>,

    pub terminal_height: u16,
    pub should_quit: bool,
    pub now_override: Option<jiff::Timestamp>,

    /// Multi-selection of PRs in the active PR list (per-repo or source-level).
    /// Keyed by `PrId` so it stays stable across re-sort/filter/refresh.
    pub selected_prs: HashSet<PrId>,
    /// Batch PR actions currently in flight. While > 0 the loading indicator is held
    /// and new batch actions are ignored so completion counting stays accurate.
    pub pending_pr_actions: u32,
    /// Dependabot PRs the open dependabot menu was invoked on. When > 1, picking a
    /// command posts it to every target; cleared as soon as the menu closes.
    pub(crate) pending_dependabot_targets: Vec<PrId>,

    /// Total targets of the in-flight batch (0 => a single-PR action, not a batch).
    /// While > 0 the loading indicator stays up and per-item status is suppressed in
    /// favor of one summary shown when `pending_pr_actions` reaches 0.
    pub(crate) batch_total: u32,
    /// How many in-flight batch items have failed so far.
    pub(crate) batch_failed: u32,
    /// Prebuilt success summary for the active batch (e.g. "approve x3" or
    /// "Sent rebase to 2 Dependabot PRs"). Shown when the batch completes cleanly.
    pub(crate) batch_summary_ok: Option<String>,

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
            direct_repo: false,
            direct_source: false,
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
            pending_clone_org: None,
            repo_cache: HashMap::new(),
            source_prs_cache: HashMap::new(),
            source_issues_cache: HashMap::new(),
            frontpage_cache: HashMap::new(),
            permission_cache: HashMap::new(),
            terminal_height: 40,
            should_quit: false,
            now_override: None,
            selected_prs: HashSet::new(),
            pending_pr_actions: 0,
            pending_dependabot_targets: Vec::new(),
            batch_total: 0,
            batch_failed: 0,
            batch_summary_ok: None,
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
        // Clear the dependabot menu's pending targets (only read right after a fresh open).
        // selected_prs and the batch completion counters are intentionally preserved so an
        // in-flight PR action/comment batch can still finish and clear loading.
        self.pending_dependabot_targets.clear();
        self.pending_clone_org = None;
        self.repo_ctx.diff_view = None;
        self.should_quit = false;
        self
    }

    /// Bootstraps state for `ghview owner/repo`: seeds a single-element source/repo
    /// selection (everything else derives the active repo from these lists) and jumps
    /// straight into the repo workspace, skipping Sources/Repos browsing entirely.
    pub fn enter_direct_repo(&mut self, repo: RepoId) {
        self.direct_repo = true;
        self.direct_source = true;
        self.sources = vec![Source::User(repo.owner.clone())];
        self.source_state.select(Some(0));
        // has_issues/has_pull_requests optimistically true: the real repo-list fetch
        // that would normally populate these is skipped in direct mode.
        self.source_ctx.repos = vec![Repo {
            name: repo.repo.clone(),
            has_issues: true,
            has_pull_requests: true,
            ..Repo::default()
        }];
        self.source_ctx.repo_state.select(Some(0));
        self.repos_view = ReposView::RepoList;
        self.focus = Column::Repo;
        self.repo_ctx.pr_body_scroll = 0;
        self.repo_ctx.issue_body_scroll = 0;
        self.repo_ctx.repo_frontpage_scroll = 0;
        self.on_repo_changed();
    }

    /// Bootstraps state for `ghview OWNER`: resolves whether OWNER is an org
    /// or a user via the API, then seeds a single-element source and jumps
    /// straight into its Repos list (a real fetch), skipping Sources browsing.
    pub fn enter_direct_owner(&mut self, owner: String) {
        self.direct_source = true;
        self.focus = Column::Repos;
        self.loading = Some(LoadingKind::Sources);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match crate::data::fetch_owner_kind(&owner).await {
                Ok(source) => {
                    let _ = tx.send(DataMsg::Sources {
                        sources: vec![source],
                        current_user: String::new(),
                    });
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
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
        self.selected_pr()
            .is_some_and(|pr| self.merge_uses_auto_for(pr))
    }

    /// Whether a merge of `pr` should use auto-merge, honoring the global toggle and
    /// the target repo's `allow_auto_merge` flag. Per-repo view uses the fetched flag;
    /// source-level lists look the repo up in the already-loaded repos list.
    pub fn merge_uses_auto_for(&self, pr: &PR) -> bool {
        if !self.config.ui.merge_auto {
            return false;
        }
        if self.repos_view != ReposView::PrList {
            // Per-repo view: use allow_auto_merge fetched from the individual repo
            // endpoint alongside viewer permissions. Defaults false until that arrives.
            return self.repo_ctx.allow_auto_merge.unwrap_or(false);
        }
        // Source PR list: look up the PR's repo in the already-loaded repos list.
        self.source_ctx
            .repos
            .iter()
            .find(|r| r.name == pr.repo)
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

    pub fn action_permitted(&self, action: Action) -> bool {
        match self.selected_pr() {
            Some(pr) => self.action_permitted_for(pr, action),
            None => true,
        }
    }

    /// Permission check for a specific PR. `can_push` is the active repo's flag
    /// (optimistic true until fetched); in cross-repo source lists it is an
    /// approximation, so `gh` failures surface per-PR instead.
    pub fn action_permitted_for(&self, pr: &PR, action: Action) -> bool {
        let current_user = self.current_user.as_deref().unwrap_or("");
        let is_author = pr.author == current_user;
        let can_push = self.repo_ctx.viewer_can_push.unwrap_or(true);
        match action {
            Action::Approve => !is_author,
            Action::Merge | Action::CheckRerun | Action::DependabotMenu => can_push,
            Action::ClosePr | Action::ReopenPr | Action::MarkReady => can_push || is_author,
            _ => true,
        }
    }

    /// The PR list currently on screen (per-repo or source-level), with filter applied.
    pub fn active_visible_prs(&self) -> Vec<&PR> {
        if self.repos_view == ReposView::PrList {
            self.visible_source_prs()
        } else {
            self.repo_ctx.prs.iter().collect()
        }
    }

    /// Cursor row in the active PR list.
    pub fn active_pr_cursor(&self) -> Option<usize> {
        if self.repos_view == ReposView::PrList {
            self.source_ctx.source_pr_state.selected()
        } else {
            self.repo_ctx.pr_state.selected()
        }
    }

    /// Stable identity of a PR in the active list (mirrors `selected_owner_repo`).
    pub fn pr_id_of(&self, pr: &PR) -> PrId {
        let owner = self.selected_source_owner().unwrap_or_default();
        if self.repos_view == ReposView::PrList {
            let actual_owner = if pr.repo_owner.is_empty() {
                owner
            } else {
                pr.repo_owner.clone()
            };
            RepoId::new(actual_owner, pr.repo.clone()).pr(pr.number)
        } else {
            RepoId::new(owner, self.selected_repo().unwrap_or_default()).pr(pr.number)
        }
    }

    /// Identity of the cursor PR in the active list, if any.
    pub fn active_pr_id(&self) -> Option<PrId> {
        let visible = self.active_visible_prs();
        self.active_pr_cursor()
            .and_then(|i| visible.get(i))
            .map(|pr| self.pr_id_of(pr))
    }

    pub fn pr_selection_active(&self) -> bool {
        !self.selected_prs.is_empty() && self.pending_pr_actions == 0
    }

    /// SPACE: toggle the cursor row in/out of the selection.
    pub fn toggle_pr_select(&mut self) {
        let Some(idx) = self.active_pr_cursor() else {
            return;
        };
        let visible = self.active_visible_prs();
        let Some(pr) = visible.get(idx).cloned() else {
            return;
        };
        let id = self.pr_id_of(pr);
        if !self.selected_prs.remove(&id) {
            self.selected_prs.insert(id);
        }
    }

    /// A: select every visible PR; press again to clear.
    pub fn select_all_prs(&mut self) {
        let visible = self.active_visible_prs();
        if visible.is_empty() {
            return;
        }
        let ids: Vec<PrId> = visible.iter().map(|pr| self.pr_id_of(pr)).collect();
        let all_selected = ids.iter().all(|id| self.selected_prs.contains(id));
        if all_selected {
            self.selected_prs.clear();
        } else {
            for id in ids {
                self.selected_prs.insert(id);
            }
        }
    }

    pub fn clear_pr_selection(&mut self) {
        self.selected_prs.clear();
    }

    /// Targets for a PR action: the whole selection when it is active (filtered
    /// per-PR by permission), otherwise just the cursor PR.
    pub fn action_targets(&self, action: Action) -> Vec<PrId> {
        if self.pr_selection_active() {
            let visible = self.active_visible_prs();
            return self
                .selected_prs
                .iter()
                .filter_map(|id| {
                    visible
                        .iter()
                        .find(|pr| &self.pr_id_of(pr) == id)
                        .filter(|pr| self.action_permitted_for(pr, action))
                        .map(|_| id.clone())
                })
                .collect();
        }
        self.selected_pr_id().into_iter().collect()
    }

    /// Whether the whole selection is active for list actions (toggle/select-all/Esc).
    pub fn pr_list_focused(&self) -> bool {
        (self.focus == Column::Repo && self.repo_view == RepoView::Prs)
            || (self.focus == Column::Repos && self.repos_view == ReposView::PrList)
    }

    pub fn is_dependabot_pr(pr: &PR) -> bool {
        matches!(pr.author.as_str(), "dependabot[bot]" | "dependabot")
    }

    pub fn selected_pr_is_dependabot(&self) -> bool {
        self.selected_pr().is_some_and(Self::is_dependabot_pr)
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

            Action::ToggleSelect => {
                if self.pr_list_focused() {
                    self.toggle_pr_select();
                }
            }
            Action::SelectAll => {
                if self.pr_list_focused() {
                    self.select_all_prs();
                }
            }
            Action::ClearSelection => {
                if self.pr_list_focused() && !self.filter_active {
                    self.clear_pr_selection();
                }
            }

            Action::Approve => self.do_pr_action_batch(PrAction::Approve),
            Action::Merge => self.do_pr_action_batch(PrAction::Merge),
            Action::ClosePr => self.do_pr_action_batch(PrAction::Close),
            Action::ReopenPr => self.do_pr_action_batch(PrAction::Reopen),
            Action::MarkReady => self.do_pr_action_batch(PrAction::MarkReady),
            Action::DependabotMenu => {
                let targets = self.dependabot_targets();
                match targets.len() {
                    0 => self.set_error("Not a Dependabot PR".to_string()),
                    _ => {
                        self.pending_dependabot_targets = targets;
                        self.show_dependabot_menu = true;
                    }
                }
            }
            Action::Diff => self.trigger_load_diff(),

            Action::CheckOpen => self.open_selected_check(),
            Action::CheckRerun => self.rerun_selected_check(),

            Action::Checkout | Action::Comment | Action::Clone => {}
        }
    }

    /// Dependabot PRs the menu should act on: the whole selection when it is active,
    /// otherwise just the cursor PR. Non-Dependabot rows are filtered out.
    pub fn dependabot_targets(&self) -> Vec<PrId> {
        let visible = self.active_visible_prs();
        if self.pr_selection_active() {
            return self
                .selected_prs
                .iter()
                .filter_map(|id| {
                    visible
                        .iter()
                        .find(|pr| &self.pr_id_of(pr) == id)
                        .filter(|pr| Self::is_dependabot_pr(pr))
                        .map(|_| id.clone())
                })
                .collect();
        }
        self.active_pr_id()
            .filter(|_| self.selected_pr_is_dependabot())
            .into_iter()
            .collect()
    }

    pub fn handle_dependabot_key(&mut self, key: char) -> bool {
        self.show_dependabot_menu = false;
        let Some((_, cmd)) = DEPENDABOT_COMMANDS.iter().find(|(k, _)| *k == key) else {
            return true;
        };
        let body = format!("@dependabot {cmd}");
        let targets = std::mem::take(&mut self.pending_dependabot_targets);
        if targets.len() > 1 {
            let n = targets.len();
            let summary = format!("Sent {cmd} to {n} Dependabot PR(s)");
            self.post_batch_comment(targets, &body, summary);
        } else if let Some(pr_id) = targets.first() {
            // Single target: post to that specific PR (a selected dependabot, not just the
            // cursor) and show a per-PR status.
            self.post_dependabot_comment(pr_id, &body);
        }
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

    #[tokio::test]
    async fn enter_direct_repo_seeds_single_source_and_repo() {
        let mut app = make_app();
        app.enter_direct_repo(RepoId::new("owner", "repo"));
        assert_eq!(app.sources.len(), 1);
        assert_eq!(app.sources[0].owner(), "owner");
        assert_eq!(app.source_state.selected(), Some(0));
        assert_eq!(app.source_ctx.repos.len(), 1);
        assert_eq!(app.source_ctx.repos[0].name, "repo");
        assert_eq!(app.source_ctx.repo_state.selected(), Some(0));
    }

    #[tokio::test]
    async fn enter_direct_repo_sets_flags() {
        let mut app = make_app();
        app.enter_direct_repo(RepoId::new("owner", "repo"));
        assert!(app.direct_repo);
        assert_eq!(app.focus, Column::Repo);
        assert_eq!(app.repos_view, ReposView::RepoList);
    }

    #[tokio::test]
    async fn enter_direct_owner_sets_flags() {
        let mut app = make_app();
        app.enter_direct_owner("someowner".to_string());
        assert!(app.direct_source);
        assert!(!app.direct_repo);
        assert_eq!(app.focus, Column::Repos);
        assert_eq!(app.loading, Some(LoadingKind::Sources));
    }

    #[tokio::test]
    async fn enter_direct_repo_resolves_owner_repo() {
        let mut app = make_app();
        app.enter_direct_repo(RepoId::new("owner", "repo"));
        assert_eq!(
            app.selected_owner_repo(),
            Some(RepoId::new("owner", "repo"))
        );
    }

    #[tokio::test]
    async fn enter_direct_repo_forces_repo_list_view_over_config_default() {
        let mut config = Config::default();
        config.ui.default_repos_view = ReposView::PrList;
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(tx, config);
        app.enter_direct_repo(RepoId::new("owner", "repo"));
        assert_eq!(app.repos_view, ReposView::RepoList);
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

    /// Focus the per-repo PR list so selection helpers operate on `repo_ctx.prs`.
    fn focus_pr_list(app: &mut App) {
        app.focus = Column::Repo;
        app.repo_view = RepoView::Prs;
    }

    #[test]
    fn toggle_pr_select_adds_then_removes_cursor() {
        let mut app = make_app();
        setup_selected_repo(&mut app);
        focus_pr_list(&mut app);
        app.repo_ctx.prs = vec![make_pr_numbered(1), make_pr_numbered(2)];
        app.repo_ctx.pr_state.select(Some(0));

        assert!(app.selected_prs.is_empty());
        app.toggle_pr_select();
        assert_eq!(app.selected_prs.len(), 1);
        app.toggle_pr_select();
        assert!(app.selected_prs.is_empty());
    }

    #[test]
    fn toggle_pr_select_is_independent_per_row() {
        let mut app = make_app();
        setup_selected_repo(&mut app);
        focus_pr_list(&mut app);
        app.repo_ctx.prs = vec![make_pr_numbered(1), make_pr_numbered(2)];
        app.repo_ctx.pr_state.select(Some(0));
        app.toggle_pr_select();
        app.repo_ctx.pr_state.select(Some(1));
        app.toggle_pr_select();
        assert_eq!(app.selected_prs.len(), 2);
    }

    #[test]
    fn select_all_selects_then_clears() {
        let mut app = make_app();
        setup_selected_repo(&mut app);
        focus_pr_list(&mut app);
        app.repo_ctx.prs = vec![
            make_pr_numbered(1),
            make_pr_numbered(2),
            make_pr_numbered(3),
        ];
        app.repo_ctx.pr_state.select(Some(0));

        app.select_all_prs();
        assert_eq!(app.selected_prs.len(), 3);
        app.select_all_prs();
        assert!(app.selected_prs.is_empty());
    }

    #[test]
    fn clear_pr_selection_empties() {
        let mut app = make_app();
        setup_selected_repo(&mut app);
        focus_pr_list(&mut app);
        app.repo_ctx.prs = vec![make_pr_numbered(1), make_pr_numbered(2)];
        app.repo_ctx.pr_state.select(Some(0));
        app.toggle_pr_select();
        assert!(!app.selected_prs.is_empty());
        app.clear_pr_selection();
        assert!(app.selected_prs.is_empty());
    }

    #[test]
    fn selection_active_false_while_action_pending() {
        let mut app = make_app();
        setup_selected_repo(&mut app);
        focus_pr_list(&mut app);
        app.repo_ctx.prs = vec![make_pr_numbered(1)];
        app.repo_ctx.pr_state.select(Some(0));
        app.toggle_pr_select();
        assert!(app.pr_selection_active());
        app.pending_pr_actions = 1;
        assert!(!app.pr_selection_active());
    }

    #[test]
    fn action_targets_returns_cursor_when_no_selection() {
        let mut app = make_app();
        setup_selected_repo(&mut app);
        focus_pr_list(&mut app);
        app.repo_ctx.prs = vec![make_pr_numbered(1), make_pr_numbered(2)];
        app.repo_ctx.pr_state.select(Some(1));

        let targets = app.action_targets(Action::Approve);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].number, 2);
    }

    #[test]
    fn action_targets_returns_selection_when_active() {
        let mut app = make_app();
        setup_selected_repo(&mut app);
        focus_pr_list(&mut app);
        app.repo_ctx.prs = vec![make_pr_numbered(1), make_pr_numbered(2)];
        app.repo_ctx.pr_state.select(Some(0));
        app.toggle_pr_select(); // selects #1

        let targets = app.action_targets(Action::Approve);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].number, 1);
    }

    #[test]
    fn action_targets_skips_own_prs_for_approve() {
        let mut app = make_app();
        setup_selected_repo(&mut app);
        focus_pr_list(&mut app);
        app.current_user = Some("alice".into());
        let mut own = make_pr_numbered(1);
        own.author = "alice".into(); // author cannot approve their own PR
        let mut other = make_pr_numbered(2);
        other.author = "bob".into();
        app.repo_ctx.prs = vec![own, other];
        app.repo_ctx.pr_state.select(Some(0));
        app.toggle_pr_select(); // #1 (own)
        app.repo_ctx.pr_state.select(Some(1));
        app.toggle_pr_select(); // #2 (other)

        let targets = app.action_targets(Action::Approve);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].number, 2);
    }

    #[test]
    fn finish_pr_action_result_counts_down_and_clears() {
        let mut app = make_app();
        setup_selected_repo(&mut app);
        focus_pr_list(&mut app);
        app.pending_pr_actions = 2;
        app.batch_total = 2;
        app.batch_failed = 0;
        app.batch_summary_ok = Some("Merged".into());

        app.finish_pr_action_result(true);
        assert_eq!(app.pending_pr_actions, 1);

        app.finish_pr_action_result(true);
        assert_eq!(app.pending_pr_actions, 0);
        // Batch summary shown on completion.
        assert!(
            app.status_msg
                .as_ref()
                .is_some_and(|(m, _)| m.contains("Merged"))
        );
        assert_eq!(app.batch_total, 0);
    }

    #[test]
    fn finish_pr_action_result_counts_failures() {
        let mut app = make_app();
        setup_selected_repo(&mut app);
        focus_pr_list(&mut app);
        app.pending_pr_actions = 2;
        app.batch_total = 2;
        app.batch_failed = 0;
        app.batch_summary_ok = None;

        app.finish_pr_action_result(true);
        app.finish_pr_action_result(false);
        assert_eq!(app.pending_pr_actions, 0);
        let (msg, is_err) = app.status_msg.clone().expect("status set");
        assert!(msg.contains("failed 1"));
        // A partial failure is a warning, not a hard error.
        assert!(!is_err);
    }

    #[test]
    fn dependabot_targets_uses_selection_when_active() {
        let mut app = make_app();
        setup_selected_repo(&mut app);
        focus_pr_list(&mut app);
        let mut d1 = make_pr_numbered(1);
        d1.author = "dependabot[bot]".into();
        let mut d2 = make_pr_numbered(2);
        d2.author = "dependabot[bot]".into();
        let mut plain = make_pr_numbered(3);
        plain.author = "alice".into();
        app.repo_ctx.prs = vec![d1, d2, plain];
        // Select both dependabot PRs.
        app.repo_ctx.pr_state.select(Some(0));
        app.toggle_pr_select();
        app.repo_ctx.pr_state.select(Some(1));
        app.toggle_pr_select();

        let targets = app.dependabot_targets();
        assert_eq!(targets.len(), 2);
    }

    #[test]
    fn single_action_failure_decrements_counter_and_shows_error() {
        let mut app = make_app();
        setup_selected_repo(&mut app);
        focus_pr_list(&mut app);
        // Simulate a lone single-PR action in flight (batch_total == 0).
        app.pending_pr_actions = 1;
        let pr = RepoId::new("owner", "repo").pr(42);
        app.handle_data(DataMsg::PrActionError {
            pr,
            msg: "boom".into(),
        });
        assert_eq!(app.pending_pr_actions, 0);
        let (msg, is_err) = app.status_msg.clone().expect("error shown");
        assert!(is_err);
        assert!(msg.contains("boom"));
    }

    #[test]
    fn dependabot_targets_empty_for_non_dependabot_cursor() {
        let mut app = make_app();
        setup_selected_repo(&mut app);
        focus_pr_list(&mut app);
        let mut plain = make_pr_numbered(1);
        plain.author = "alice".into();
        app.repo_ctx.prs = vec![plain];
        app.repo_ctx.pr_state.select(Some(0));

        assert!(app.dependabot_targets().is_empty());
    }
}
