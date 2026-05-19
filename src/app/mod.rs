mod event_loop;
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
        MergeableState, PR, PrAction, Repo, RepoSortKey, RepoView, ReposView, ReviewStatus,
        SortKey, Source,
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
    pub mergeable_states: HashMap<(String, String, u64), MergeableState>,
    pub check_summary_cache: HashMap<(String, String, u64), CheckStatus>,
    pub issues: Vec<Issue>,
    pub issue_state: ListState,
    pub issues_pagination: PaginationState,
    pub issue_body: Option<String>,
    pub issue_body_scroll: u16,
    pub repo_frontpage: Option<(String, String)>,
    pub repo_frontpage_scroll: u16,
    pub viewer_can_push: Option<bool>,
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
}

pub struct App {
    pub focus: Column,

    pub sources: Vec<Source>,
    pub source_state: ListState,
    pub source_filter: String,
    pub current_user: Option<String>,

    pub repo_ctx: RepoCtx,
    pub source_ctx: SourceCtx,

    pub repo_cache: HashMap<String, (Instant, Vec<Repo>)>,
    pub pr_filter: String,
    pub pr_cache: HashMap<String, (Instant, Vec<PR>)>,

    pub(crate) review_cache: HashMap<String, HashMap<u64, ReviewStatus>>,

    pub filter_active: bool,
    pub sort_key: SortKey,
    pub repo_sort_key: RepoSortKey,

    pub rate_limit: Option<(u32, u32)>,

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

    pub terminal_height: u16,
    pub should_quit: bool,
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
            loading: None,
            config,
            status_msg: None,
            status_msg_at: None,
            show_help: false,
            help_scroll: 0,
            show_dependabot_menu: false,
            repo_cache: HashMap::new(),
            source_prs_cache: HashMap::new(),
            terminal_height: 40,
            should_quit: false,
            tx,
        }
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

    pub fn selected_repo_has_issues(&self) -> bool {
        let vr = self.visible_repos();
        self.source_ctx
            .repo_state
            .selected()
            .and_then(|i| vr.get(i))
            .is_none_or(|r| r.has_issues)
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

    pub(crate) fn selected_pr_context(&self) -> Option<(String, String, PR)> {
        let (owner, repo) = self.selected_owner_repo()?;
        let pr = self.selected_pr()?.clone();
        Some((owner, repo, pr))
    }

    pub fn selected_issue(&self) -> Option<&Issue> {
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

    pub fn handle_data(&mut self, msg: DataMsg) {
        match msg {
            DataMsg::Sources {
                sources,
                current_user,
            } => {
                self.current_user = Some(current_user);
                self.sources = sources;
                self.clamp_source_selection();
                if self.source_state.selected().is_none() && !self.visible_sources().is_empty() {
                    self.source_state.select(Some(0));
                }
                if self.source_state.selected().is_some() {
                    self.trigger_load_repos();
                } else {
                    self.loading = None;
                }
            }
            DataMsg::Repos {
                owner,
                repos,
                has_more,
            } => {
                if self.selected_source_owner().as_deref() != Some(&owner) {
                    return;
                }
                self.repo_cache
                    .insert(owner, (Instant::now(), repos.clone()));
                self.source_ctx.repos_pagination.reset(has_more);
                self.apply_repos(repos);
                if self.source_ctx.repo_state.selected().is_some() {
                    self.trigger_load_prs();
                } else {
                    self.loading = None;
                }
            }
            DataMsg::MoreRepos {
                owner,
                repos,
                has_more,
            } => {
                if self.selected_source_owner().as_deref() != Some(&owner) {
                    return;
                }
                self.source_ctx.repos_pagination.finish(has_more);
                self.source_ctx.repos.extend(repos);
                self.sort_repos_in_place();
                self.loading = None;
            }
            DataMsg::Prs {
                owner,
                repo,
                prs,
                has_more,
            } => {
                let key = format!("{owner}/{repo}");
                let is_current = self.current_repo_key().as_deref() == Some(&key);
                if is_current {
                    self.repo_ctx.prs_pagination.reset(has_more);
                    self.pr_cache.insert(key, (Instant::now(), prs.clone()));
                    self.apply_prs(prs);
                    self.loading = None;
                } else {
                    self.pr_cache.insert(key, (Instant::now(), prs));
                }
            }
            DataMsg::MorePrs {
                owner,
                repo,
                prs,
                has_more,
            } => {
                let key = format!("{owner}/{repo}");
                if self.current_repo_key().as_deref() != Some(&key) {
                    return;
                }
                self.repo_ctx.prs_pagination.finish(has_more);
                self.repo_ctx.prs_raw.extend(prs);
                self.rebuild_prs();
                self.loading = None;
            }
            DataMsg::ReviewStatus {
                owner,
                repo,
                pr_number,
                status,
            } => {
                let key = format!("{owner}/{repo}");
                let is_current = self.current_repo_key().as_deref() == Some(&key);
                self.review_cache
                    .entry(key)
                    .or_default()
                    .insert(pr_number, status);
                if is_current {
                    self.repo_ctx.review_statuses.insert(pr_number, status);
                }
            }
            DataMsg::CheckRuns {
                owner,
                repo,
                pr_number,
                mut runs,
            } => {
                let key = format!("{owner}/{repo}");
                let passes = if self.repos_view == ReposView::PrList {
                    self.selected_source_owner().as_deref() == Some(&owner)
                        && self.source_ctx.source_prs.iter().any(|pr| pr.repo == repo)
                } else {
                    self.current_repo_key().as_deref() == Some(&key)
                };
                if !passes {
                    return;
                }
                runs.sort_by_key(|r| r.status != crate::types::CheckStatus::Failing);
                let summary = if runs.is_empty() {
                    CheckStatus::Unknown
                } else if runs.iter().any(|r| r.status == CheckStatus::Failing) {
                    CheckStatus::Failing
                } else if runs.iter().any(|r| r.status == CheckStatus::Pending) {
                    CheckStatus::Pending
                } else if runs.iter().all(|r| r.status == CheckStatus::Passing) {
                    CheckStatus::Passing
                } else {
                    CheckStatus::Unknown
                };
                self.repo_ctx
                    .check_summary_cache
                    .insert((owner.clone(), repo.clone(), pr_number), summary);
                if self.selected_pr().is_some_and(|pr| {
                    pr.number == pr_number && (pr.repo.is_empty() || pr.repo == repo)
                }) {
                    self.repo_ctx.check_runs = Some(runs);
                    if !self.checks_focusable()
                        && self.focus == Column::Detail
                        && self.repo_ctx.detail_section == DetailSection::Checks
                    {
                        self.repo_ctx.detail_section = DetailSection::Body;
                    }
                }
            }
            DataMsg::DiffContent { title, content } => {
                self.repo_ctx.diff_view = Some(DiffView {
                    title,
                    lines: content
                        .lines()
                        .map(std::string::ToString::to_string)
                        .collect(),
                    scroll: 0,
                });
                self.loading = None;
            }
            DataMsg::PrBody {
                owner,
                repo,
                pr_number,
                body,
                mergeable_state,
                additions,
                deletions,
            } => {
                let key = format!("{owner}/{repo}");
                let passes = if self.repos_view == ReposView::PrList {
                    self.selected_source_owner().as_deref() == Some(&owner)
                        && self.source_ctx.source_prs.iter().any(|pr| pr.repo == repo)
                } else {
                    self.current_repo_key().as_deref() == Some(&key)
                };
                if !passes {
                    return;
                }
                self.repo_ctx
                    .mergeable_states
                    .insert((owner.clone(), repo.clone(), pr_number), mergeable_state);
                if self.selected_pr().is_some_and(|pr| {
                    pr.number == pr_number && (pr.repo.is_empty() || pr.repo == repo)
                }) {
                    self.repo_ctx.pr_body = Some(body);
                    if !self.pr_body_focusable() && self.focus == Column::Detail {
                        self.repo_ctx.detail_section = DetailSection::Checks;
                    }
                }
                for list in [&mut self.repo_ctx.prs_raw, &mut self.repo_ctx.prs] {
                    if let Some(pr) = list.iter_mut().find(|p| p.number == pr_number) {
                        pr.additions = additions;
                        pr.deletions = deletions;
                    }
                }
                if let Some(spr) = self
                    .source_ctx
                    .source_prs
                    .iter_mut()
                    .find(|p| p.repo == repo && p.number == pr_number)
                {
                    spr.additions = additions;
                    spr.deletions = deletions;
                }
            }
            DataMsg::RepoFrontpage {
                owner,
                repo,
                description,
                readme,
            } => {
                let key = format!("{owner}/{repo}");
                if self.current_repo_key().as_deref() == Some(&key) {
                    self.repo_ctx.repo_frontpage = Some((description, readme));
                }
            }
            DataMsg::Issues {
                owner,
                repo,
                issues,
                has_more,
            } => {
                let key = format!("{owner}/{repo}");
                if self.current_repo_key().as_deref() == Some(&key) {
                    self.repo_ctx.issues_pagination.reset(has_more);
                    self.repo_ctx.issues = issues;
                    if !self.repo_ctx.issues.is_empty()
                        && self.repo_ctx.issue_state.selected().is_none()
                    {
                        self.repo_ctx.issue_state.select(Some(0));
                        self.trigger_load_issue_body();
                    }
                    self.loading = None;
                }
            }
            DataMsg::MoreIssues {
                owner,
                repo,
                issues,
                has_more,
            } => {
                let key = format!("{owner}/{repo}");
                if self.current_repo_key().as_deref() != Some(&key) {
                    return;
                }
                self.repo_ctx.issues_pagination.finish(has_more);
                self.repo_ctx.issues.extend(issues);
                self.loading = None;
            }
            DataMsg::IssueBody {
                owner,
                repo,
                number,
                body,
            } => {
                let key = format!("{owner}/{repo}");
                if self.current_repo_key().as_deref() != Some(&key) {
                    return;
                }
                if self.selected_issue().is_some_and(|i| i.number == number) {
                    self.repo_ctx.issue_body = Some(body);
                }
            }
            DataMsg::RateLimit { remaining, limit } => {
                self.rate_limit = Some((remaining, limit));
            }
            DataMsg::ViewerPermission {
                owner,
                repo,
                can_push,
            } => {
                if self.current_repo_key().as_deref() == Some(&format!("{owner}/{repo}")) {
                    self.repo_ctx.viewer_can_push = Some(can_push);
                }
            }
            DataMsg::SourcePrs {
                owner,
                prs,
                has_more,
            } => {
                if self.selected_source_owner().as_deref() != Some(&owner) {
                    return;
                }
                self.source_ctx.source_prs_pagination.reset(has_more);
                self.source_prs_cache
                    .insert(owner, (Instant::now(), prs.clone()));
                self.apply_source_prs(prs);
                self.loading = None;
            }
            DataMsg::MoreSourcePrs {
                owner,
                prs,
                has_more,
            } => {
                if self.selected_source_owner().as_deref() != Some(&owner) {
                    return;
                }
                self.source_ctx.source_prs_pagination.finish(has_more);
                self.source_ctx.source_prs.extend(prs);
                self.loading = None;
            }
            DataMsg::ActionDone(msg) => {
                if let Some(m) = msg {
                    self.set_status(m);
                }
                self.loading = None;
            }
            DataMsg::Error(e) => {
                self.set_status(format!("Error: {e}"));
                self.loading = None;
            }
        }
    }

    pub(crate) fn current_repo_key(&self) -> Option<String> {
        let (owner, repo) = self.selected_owner_repo()?;
        Some(format!("{owner}/{repo}"))
    }

    /// Clear all state scoped to a single repo. Call this whenever the active repo changes.
    /// Add new per-repo caches here so every transition site is covered automatically.
    pub(crate) fn invalidate_repo(&mut self) {
        self.repo_ctx = RepoCtx::default();
    }

    /// Clear all state scoped to a single source. Calls invalidate_repo(), then clears
    /// source-level lists. Add new per-source caches here.
    pub(crate) fn invalidate_source(&mut self) {
        self.repo_ctx = RepoCtx::default();
        self.source_ctx = SourceCtx::default();
    }

    pub(crate) fn apply_repos(&mut self, repos: Vec<Repo>) {
        self.source_ctx.repos = repos;
        self.sort_repos_in_place();
        self.invalidate_repo();
        self.clamp_repo_selection();
        if self.source_ctx.repo_state.selected().is_none() && !self.visible_repos().is_empty() {
            self.source_ctx.repo_state.select(Some(0));
        }
    }

    pub(crate) fn apply_prs(&mut self, prs: Vec<PR>) {
        self.repo_ctx.prs_raw = prs;
        self.rebuild_prs();
        if let Some(key) = self.current_repo_key() {
            match self.review_cache.get(&key) {
                Some(m) => self.repo_ctx.review_statuses.clone_from(m),
                None => self.repo_ctx.review_statuses.clear(),
            }
        }
        self.trigger_prefetch_pr_details();
    }

    pub(crate) fn apply_source_prs(&mut self, prs: Vec<PR>) {
        let was_empty = self.source_ctx.source_prs.is_empty();
        self.source_ctx.source_prs = prs;
        if self.source_ctx.source_pr_state.selected().is_none()
            && !self.source_ctx.source_prs.is_empty()
        {
            self.source_ctx.source_pr_state.select(Some(0));
        }
        if was_empty && !self.source_ctx.source_prs.is_empty() {
            self.trigger_load_pr_body();
        }
        self.trigger_review_and_check_fetches();
        self.trigger_prefetch_pr_details();
    }

    fn sort_repos_in_place(&mut self) {
        match self.repo_sort_key {
            RepoSortKey::RecentlyUpdated => {
                self.source_ctx
                    .repos
                    .sort_by(|a, b| b.pushed_at.cmp(&a.pushed_at));
            }
            RepoSortKey::Alphabetical => {
                self.source_ctx
                    .repos
                    .sort_by_cached_key(|r| r.name.to_lowercase());
            }
            RepoSortKey::Created => {
                self.source_ctx
                    .repos
                    .sort_by(|a, b| b.created_at.cmp(&a.created_at));
            }
        }
    }

    pub(crate) fn rebuild_prs(&mut self) {
        let filter = self.pr_filter.to_lowercase();
        if filter.is_empty() {
            self.repo_ctx.prs.clone_from(&self.repo_ctx.prs_raw);
        } else {
            self.repo_ctx.prs.clear();
            self.repo_ctx.prs.extend(
                self.repo_ctx
                    .prs_raw
                    .iter()
                    .filter(|pr| {
                        pr.title.to_lowercase().contains(&filter)
                            || pr.author.to_lowercase().contains(&filter)
                            || pr.labels.iter().any(|l| l.to_lowercase().contains(&filter))
                            || pr.head_ref.to_lowercase().contains(&filter)
                    })
                    .cloned(),
            );
        }
        self.apply_sort_in_place();
        let len = self.repo_ctx.prs.len();
        match self.repo_ctx.pr_state.selected() {
            Some(i) if i >= len => {
                self.repo_ctx
                    .pr_state
                    .select(if len > 0 { Some(len - 1) } else { None })
            }
            None if len > 0 => self.repo_ctx.pr_state.select(Some(0)),
            _ => {}
        }
        self.repo_ctx.pr_body_scroll = 0;
    }

    fn apply_sort_in_place(&mut self) {
        match self.sort_key {
            SortKey::Newest => self
                .repo_ctx
                .prs
                .sort_by(|a, b| b.created_at.cmp(&a.created_at)),
            SortKey::RecentlyUpdated => self
                .repo_ctx
                .prs
                .sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
            SortKey::Oldest => self
                .repo_ctx
                .prs
                .sort_by(|a, b| a.created_at.cmp(&b.created_at)),
            SortKey::LeastReviewed => {
                let review_priority = |pr: &PR| -> u8 {
                    match self.repo_ctx.review_statuses.get(&pr.number) {
                        None | Some(ReviewStatus::Unknown) | Some(ReviewStatus::Pending) => 0,
                        Some(ReviewStatus::ChangesRequested) => 1,
                        Some(ReviewStatus::Approved) => 2,
                    }
                };
                self.repo_ctx.prs.sort_by_key(|pr| review_priority(pr));
            }
        }
    }

    pub fn handle_filter_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                *self.active_filter_mut() = String::new();
                self.filter_active = false;
                self.clamp_source_selection();
                self.clamp_repo_selection();
                self.clamp_source_pr_selection();
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
                self.rebuild_prs();
            }
            KeyCode::Char(c) => {
                self.active_filter_mut().push(c);
                self.clamp_source_selection();
                self.clamp_repo_selection();
                self.clamp_source_pr_selection();
                self.rebuild_prs();
            }
            _ => {}
        }
    }

    fn active_filter_mut(&mut self) -> &mut String {
        match self.focus {
            Column::Sources | Column::Detail => &mut self.source_filter,
            Column::Repos => {
                if self.repos_view == ReposView::PrList {
                    &mut self.source_ctx.source_pr_filter
                } else {
                    &mut self.source_ctx.repo_filter
                }
            }
            Column::Repo => &mut self.pr_filter,
        }
    }

    pub fn active_filter(&self) -> &str {
        match self.focus {
            Column::Sources | Column::Detail => &self.source_filter,
            Column::Repos => {
                if self.repos_view == ReposView::PrList {
                    &self.source_ctx.source_pr_filter
                } else {
                    &self.source_ctx.repo_filter
                }
            }
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
                if self.focus == Column::Repos {
                    self.repo_sort_key = self.repo_sort_key.next();
                    self.sort_repos_in_place();
                    if !self.source_ctx.repos.is_empty() {
                        self.source_ctx.repo_state.select(Some(0));
                    }
                } else {
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
            repo: String::new(),
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
}
