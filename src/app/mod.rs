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
        MergeableState, PR, PrAction, Repo, RepoSortKey, RepoView, ReviewStatus, SortKey, Source,
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

pub struct App {
    pub focus: Column,

    pub sources: Vec<Source>,
    pub source_state: ListState,
    pub source_filter: String,
    pub current_user: Option<String>,
    pub viewer_can_push: Option<bool>,

    pub repos: Vec<Repo>,
    pub repo_state: ListState,
    pub repo_filter: String,

    pub repos_pagination: PaginationState,
    pub repo_cache: HashMap<String, (Instant, Vec<Repo>)>,

    pub prs_raw: Vec<PR>,
    pub prs: Vec<PR>,
    pub pr_state: ListState,
    pub pr_filter: String,
    pub pr_cache: HashMap<String, (Instant, Vec<PR>)>,

    pub review_statuses: HashMap<u64, ReviewStatus>,
    pub mergeable_states: HashMap<u64, MergeableState>,
    pub(crate) review_cache: HashMap<String, HashMap<u64, ReviewStatus>>,
    pub check_summary_cache: HashMap<u64, CheckStatus>,

    pub filter_active: bool,
    pub sort_key: SortKey,
    pub repo_sort_key: RepoSortKey,

    pub rate_limit: Option<(u32, u32)>,

    pub check_runs: Option<Vec<CheckRun>>,
    pub check_runs_state: ListState,
    pub pr_body_scroll: u16,
    pub detail_section: DetailSection,

    pub loading: Option<LoadingKind>,
    pub config: Config,
    pub status_msg: Option<(String, bool)>,
    pub(crate) status_msg_at: Option<Instant>,
    pub show_help: bool,
    pub help_scroll: u16,
    pub show_dependabot_menu: bool,
    pub diff_view: Option<DiffView>,
    pub pr_body: Option<String>,
    pub repo_view: RepoView,
    pub repo_frontpage: Option<(String, String)>,
    pub repo_frontpage_scroll: u16,

    pub prs_pagination: PaginationState,
    pub issues_pagination: PaginationState,

    pub issues: Vec<Issue>,
    pub issue_state: ListState,
    pub issue_body: Option<String>,
    pub issue_body_scroll: u16,
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
            viewer_can_push: None,
            repos: vec![],
            repo_state: ListState::default(),
            repo_filter: String::new(),
            prs_raw: vec![],
            prs: vec![],
            pr_state: ListState::default(),
            pr_filter: String::new(),
            pr_cache: HashMap::new(),
            review_statuses: HashMap::new(),
            mergeable_states: HashMap::new(),
            review_cache: HashMap::new(),
            check_summary_cache: HashMap::new(),
            filter_active: false,
            sort_key: SortKey::Newest,
            repo_sort_key: config.ui.repo_sort,
            repo_view: config.ui.default_repo_view,
            rate_limit: None,
            check_runs: None,
            check_runs_state: ListState::default(),
            pr_body_scroll: 0,
            detail_section: DetailSection::default(),
            loading: None,
            config,
            status_msg: None,
            status_msg_at: None,
            show_help: false,
            help_scroll: 0,
            show_dependabot_menu: false,
            diff_view: None,
            pr_body: None,
            repo_frontpage: None,
            repo_frontpage_scroll: 0,
            repos_pagination: PaginationState::default(),
            repo_cache: HashMap::new(),
            prs_pagination: PaginationState::default(),
            issues_pagination: PaginationState::default(),
            issues: vec![],
            issue_state: ListState::default(),
            issue_body: None,
            issue_body_scroll: 0,
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
        self.diff_view = None;
        self.should_quit = false;
        self
    }

    pub fn visible_sources(&self) -> Vec<&Source> {
        filter_visible(&self.sources, &self.source_filter, |s, f| {
            s.owner().to_lowercase().contains(f)
        })
    }

    pub fn visible_repos(&self) -> Vec<&Repo> {
        filter_visible(&self.repos, &self.repo_filter, |r, f| {
            r.name.to_lowercase().contains(f)
        })
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
        self.repo_state
            .selected()
            .and_then(|i| vr.get(i).map(|r| r.name.as_str()))
    }

    pub fn selected_repo_has_issues(&self) -> bool {
        let vr = self.visible_repos();
        self.repo_state
            .selected()
            .and_then(|i| vr.get(i))
            .is_none_or(|r| r.has_issues)
    }

    pub fn selected_pr(&self) -> Option<&PR> {
        self.pr_state.selected().and_then(|i| self.prs.get(i))
    }

    pub(crate) fn selected_pr_context(&self) -> Option<(String, String, PR)> {
        let owner = self.selected_source_owner()?;
        let repo = self.selected_repo()?.to_string();
        let pr = self.selected_pr()?.clone();
        Some((owner, repo, pr))
    }

    pub fn selected_issue(&self) -> Option<&Issue> {
        self.issue_state.selected().and_then(|i| self.issues.get(i))
    }

    pub fn pr_body_focusable(&self) -> bool {
        self.pr_body.as_deref() != Some("")
    }

    pub fn checks_focusable(&self) -> bool {
        self.check_runs.as_ref().is_none_or(|runs| !runs.is_empty())
    }

    pub fn action_permitted(&self, action: Action) -> bool {
        let pr = self.selected_pr();
        let current_user = self.current_user.as_deref().unwrap_or("");
        let is_author = pr.is_some_and(|p| p.author == current_user);
        let can_push = self.viewer_can_push.unwrap_or(true);
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
                self.repos_pagination.reset(has_more);
                self.apply_repos(repos);
                if self.repo_state.selected().is_some() {
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
                self.repos_pagination.finish(has_more);
                self.repos.extend(repos);
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
                    self.prs_pagination.reset(has_more);
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
                self.prs_pagination.finish(has_more);
                self.prs_raw.extend(prs);
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
                    self.review_statuses.insert(pr_number, status);
                }
            }
            DataMsg::CheckRuns {
                owner,
                repo,
                pr_number,
                mut runs,
            } => {
                let key = format!("{owner}/{repo}");
                if self.current_repo_key().as_deref() != Some(&key) {
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
                self.check_summary_cache.insert(pr_number, summary);
                if self.selected_pr().is_some_and(|pr| pr.number == pr_number) {
                    self.check_runs = Some(runs);
                    if !self.checks_focusable()
                        && self.focus == Column::Detail
                        && self.detail_section == DetailSection::Checks
                    {
                        self.detail_section = DetailSection::Body;
                    }
                }
            }
            DataMsg::DiffContent { title, content } => {
                self.diff_view = Some(DiffView {
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
                if self.current_repo_key().as_deref() != Some(&key) {
                    return;
                }
                self.mergeable_states.insert(pr_number, mergeable_state);
                if self.selected_pr().is_some_and(|pr| pr.number == pr_number) {
                    self.pr_body = Some(body);
                    if !self.pr_body_focusable() && self.focus == Column::Detail {
                        self.detail_section = DetailSection::Checks;
                    }
                }
                for list in [&mut self.prs_raw, &mut self.prs] {
                    if let Some(pr) = list.iter_mut().find(|p| p.number == pr_number) {
                        pr.additions = additions;
                        pr.deletions = deletions;
                    }
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
                    self.repo_frontpage = Some((description, readme));
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
                    self.issues_pagination.reset(has_more);
                    self.issues = issues;
                    if !self.issues.is_empty() && self.issue_state.selected().is_none() {
                        self.issue_state.select(Some(0));
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
                self.issues_pagination.finish(has_more);
                self.issues.extend(issues);
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
                    self.issue_body = Some(body);
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
                    self.viewer_can_push = Some(can_push);
                }
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
        let owner = self.selected_source_owner()?;
        let repo = self.selected_repo()?;
        Some(format!("{owner}/{repo}"))
    }

    pub(crate) fn clear_pr_state(&mut self) {
        self.prs_raw = vec![];
        self.prs = vec![];
        self.pr_state = ListState::default();
    }

    pub(crate) fn clear_issue_state(&mut self) {
        self.issues = vec![];
        self.issue_state = ListState::default();
        self.issue_body = None;
        self.issue_body_scroll = 0;
    }

    pub(crate) fn clear_pr_detail(&mut self) {
        self.pr_body = None;
        self.check_runs = None;
        self.check_runs_state = ListState::default();
        self.pr_body_scroll = 0;
        self.detail_section = DetailSection::default();
        self.diff_view = None;
    }

    pub(crate) fn apply_repos(&mut self, repos: Vec<Repo>) {
        self.repos = repos;
        self.sort_repos_in_place();
        self.clear_pr_state();
        self.clear_pr_detail();
        self.review_statuses.clear();
        self.mergeable_states.clear();
        self.check_summary_cache.clear();
        self.clear_issue_state();
        self.repo_frontpage = None;
        self.repo_frontpage_scroll = 0;
        self.clamp_repo_selection();
        if self.repo_state.selected().is_none() && !self.visible_repos().is_empty() {
            self.repo_state.select(Some(0));
        }
    }

    pub(crate) fn apply_prs(&mut self, prs: Vec<PR>) {
        self.prs_raw = prs;
        self.rebuild_prs();
        if let Some(key) = self.current_repo_key() {
            match self.review_cache.get(&key) {
                Some(m) => self.review_statuses.clone_from(m),
                None => self.review_statuses.clear(),
            }
        }
        self.trigger_prefetch_pr_details();
    }

    fn sort_repos_in_place(&mut self) {
        match self.repo_sort_key {
            RepoSortKey::RecentlyUpdated => {
                self.repos.sort_by(|a, b| b.pushed_at.cmp(&a.pushed_at));
            }
            RepoSortKey::Alphabetical => {
                self.repos.sort_by_cached_key(|r| r.name.to_lowercase());
            }
            RepoSortKey::Created => {
                self.repos.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            }
        }
    }

    pub(crate) fn rebuild_prs(&mut self) {
        let filter = self.pr_filter.to_lowercase();
        if filter.is_empty() {
            self.prs.clone_from(&self.prs_raw);
        } else {
            self.prs.clear();
            self.prs.extend(
                self.prs_raw
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
        let len = self.prs.len();
        match self.pr_state.selected() {
            Some(i) if i >= len => self
                .pr_state
                .select(if len > 0 { Some(len - 1) } else { None }),
            None if len > 0 => self.pr_state.select(Some(0)),
            _ => {}
        }
        self.pr_body_scroll = 0;
    }

    fn apply_sort_in_place(&mut self) {
        match self.sort_key {
            SortKey::Newest => self.prs.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
            SortKey::RecentlyUpdated => self.prs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
            SortKey::Oldest => self.prs.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
            SortKey::LeastReviewed => {
                let review_priority = |pr: &PR| -> u8 {
                    match self.review_statuses.get(&pr.number) {
                        None | Some(ReviewStatus::Unknown) | Some(ReviewStatus::Pending) => 0,
                        Some(ReviewStatus::ChangesRequested) => 1,
                        Some(ReviewStatus::Approved) => 2,
                    }
                };
                self.prs.sort_by_key(|pr| review_priority(pr));
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
                self.rebuild_prs();
            }
            KeyCode::Enter => {
                self.filter_active = false;
            }
            KeyCode::Backspace => {
                self.active_filter_mut().pop();
                self.clamp_source_selection();
                self.clamp_repo_selection();
                self.rebuild_prs();
            }
            KeyCode::Char(c) => {
                self.active_filter_mut().push(c);
                self.clamp_source_selection();
                self.clamp_repo_selection();
                self.rebuild_prs();
            }
            _ => {}
        }
    }

    fn active_filter_mut(&mut self) -> &mut String {
        match self.focus {
            Column::Sources | Column::Detail => &mut self.source_filter,
            Column::Repos => &mut self.repo_filter,
            Column::Repo => &mut self.pr_filter,
        }
    }

    pub fn active_filter(&self) -> &str {
        match self.focus {
            Column::Sources | Column::Detail => &self.source_filter,
            Column::Repos => &self.repo_filter,
            Column::Repo => &self.pr_filter,
        }
    }

    pub fn handle_action(&mut self, action: Action) {
        if self.diff_view.is_some() {
            match action {
                Action::Quit | Action::Left => self.diff_view = None,
                Action::Down | Action::Right => self.diff_scroll(3),
                Action::Up => self.diff_scroll_up(3),
                Action::Bottom => {
                    if let Some(d) = &mut self.diff_view {
                        d.scroll =
                            u16::try_from(d.lines.len().saturating_sub(1)).unwrap_or(u16::MAX);
                    }
                }
                Action::Top => {
                    if let Some(d) = &mut self.diff_view {
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
                    if !self.repos.is_empty() {
                        self.repo_state.select(Some(0));
                    }
                } else {
                    self.sort_key = self.sort_key.next();
                    self.pr_state.select(Some(0));
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
        }
    }

    #[test]
    fn dependabot_bot_is_recognized() {
        let mut app = make_app();
        app.prs = vec![make_pr("dependabot[bot]")];
        app.pr_state.select(Some(0));
        assert!(app.selected_pr_is_dependabot());
    }

    #[test]
    fn dependabot_legacy_name_recognized() {
        let mut app = make_app();
        app.prs = vec![make_pr("dependabot")];
        app.pr_state.select(Some(0));
        assert!(app.selected_pr_is_dependabot());
    }

    #[test]
    fn dependabot_prefix_only_not_recognized() {
        let mut app = make_app();
        app.prs = vec![make_pr("dependabot-hacker")];
        app.pr_state.select(Some(0));
        assert!(!app.selected_pr_is_dependabot());
    }

    #[test]
    fn regular_user_not_dependabot() {
        let mut app = make_app();
        app.prs = vec![make_pr("alice")];
        app.pr_state.select(Some(0));
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
        app.repos = vec![
            Repo {
                name: "frontend".into(),
                ..Repo::default()
            },
            Repo {
                name: "backend".into(),
                ..Repo::default()
            },
        ];
        app.repo_filter = "front".into();
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
        app.prs_raw = vec![p1, p2];
        app.pr_filter = "login".into();
        app.rebuild_prs();
        assert_eq!(app.prs.len(), 1);
        assert_eq!(app.prs[0].author, "bob");
    }

    #[test]
    fn rebuild_prs_filter_by_author() {
        let mut app = make_app();
        app.prs_raw = vec![make_pr("alice"), make_pr("bob")];
        app.pr_filter = "bob".into();
        app.rebuild_prs();
        assert_eq!(app.prs.len(), 1);
        assert_eq!(app.prs[0].author, "bob");
    }

    #[test]
    fn rebuild_prs_empty_filter_keeps_all() {
        let mut app = make_app();
        app.prs_raw = vec![make_pr("alice"), make_pr("bob")];
        app.rebuild_prs();
        assert_eq!(app.prs.len(), 2);
    }
}
