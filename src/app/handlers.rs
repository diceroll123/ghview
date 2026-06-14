use super::{App, RepoCtx, SourceCtx};
use crate::types::{
    CheckStatus, Column, DataMsg, DetailSection, DiffView, Repo, RepoSortKey, ReposView,
    ReviewStatus, SortKey, PR,
};

impl App {
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
                    .insert(owner, (std::time::Instant::now(), repos.clone()));
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
                repo,
                prs,
                has_more,
            } => {
                let key = repo.key();
                let is_current = self.current_repo_key().as_deref() == Some(&key);
                if is_current {
                    self.repo_ctx.prs_pagination.reset(has_more);
                    self.pr_cache
                        .insert(key, (std::time::Instant::now(), prs.clone()));
                    self.apply_prs(prs);
                    self.loading = None;
                } else {
                    self.pr_cache
                        .insert(key, (std::time::Instant::now(), prs));
                }
            }
            DataMsg::MorePrs {
                repo,
                prs,
                has_more,
            } => {
                if self.current_repo_key().as_deref() != Some(repo.key().as_str()) {
                    return;
                }
                self.repo_ctx.prs_pagination.finish(has_more);
                self.repo_ctx.prs_raw.extend(prs);
                self.rebuild_prs();
                self.loading = None;
            }
            DataMsg::ReviewStatus { pr, status } => {
                let key = pr.repo.key();
                let is_current = self.current_repo_key().as_deref() == Some(&key);
                self.review_cache
                    .entry(key)
                    .or_default()
                    .insert(pr.number, status);
                if is_current {
                    self.repo_ctx.review_statuses.insert(pr.number, status);
                }
            }
            DataMsg::CheckRuns { pr, mut runs } => {
                let passes = if self.repos_view == ReposView::PrList {
                    self.selected_source_owner().as_deref() == Some(&pr.repo.owner)
                        && self
                            .source_ctx
                            .source_prs
                            .iter()
                            .any(|p| p.repo == pr.repo.repo)
                } else {
                    self.current_repo_key().as_deref() == Some(pr.repo.key().as_str())
                };
                if !passes {
                    return;
                }
                runs.sort_by_key(|r| r.status != CheckStatus::Failing);
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
                    .insert(pr.clone(), summary);
                if self.selected_pr().is_some_and(|p| {
                    p.number == pr.number && (p.repo.is_empty() || p.repo == pr.repo.repo)
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
            DataMsg::DiffContent { pr, title, content } => {
                if self.current_repo_key() != Some(pr.repo.key()) {
                    return;
                }
                if self.selected_pr().is_none_or(|p| p.number != pr.number) {
                    return;
                }
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
                pr,
                body,
                mergeable_state,
                additions,
                deletions,
            } => {
                let passes = if self.repos_view == ReposView::PrList {
                    self.selected_source_owner().as_deref() == Some(&pr.repo.owner)
                        && self
                            .source_ctx
                            .source_prs
                            .iter()
                            .any(|p| p.repo == pr.repo.repo)
                } else {
                    self.current_repo_key().as_deref() == Some(pr.repo.key().as_str())
                };
                if !passes {
                    return;
                }
                self.repo_ctx
                    .mergeable_states
                    .insert(pr.clone(), mergeable_state);
                if self.selected_pr().is_some_and(|p| {
                    p.number == pr.number && (p.repo.is_empty() || p.repo == pr.repo.repo)
                }) {
                    self.repo_ctx.pr_body = Some(body);
                    if !self.pr_body_focusable() && self.focus == Column::Detail {
                        self.repo_ctx.detail_section = DetailSection::Checks;
                    }
                }
                for list in [&mut self.repo_ctx.prs_raw, &mut self.repo_ctx.prs] {
                    if let Some(p) = list.iter_mut().find(|p| p.number == pr.number) {
                        p.additions = additions;
                        p.deletions = deletions;
                    }
                }
                if let Some(spr) = self
                    .source_ctx
                    .source_prs
                    .iter_mut()
                    .find(|p| p.repo == pr.repo.repo && p.number == pr.number)
                {
                    spr.additions = additions;
                    spr.deletions = deletions;
                }
            }
            DataMsg::RepoFrontpage {
                repo,
                description,
                readme,
            } => {
                if self.current_repo_key().as_deref() == Some(repo.key().as_str()) {
                    self.repo_ctx.repo_frontpage = Some((description, readme));
                }
            }
            DataMsg::Issues {
                repo,
                issues,
                has_more,
            } => {
                if self.current_repo_key().as_deref() == Some(repo.key().as_str()) {
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
                repo,
                issues,
                has_more,
            } => {
                if self.current_repo_key().as_deref() != Some(repo.key().as_str()) {
                    return;
                }
                self.repo_ctx.issues_pagination.finish(has_more);
                self.repo_ctx.issues.extend(issues);
                self.loading = None;
            }
            DataMsg::IssueBody { repo, number, body } => {
                if self.current_repo_key().as_deref() != Some(repo.key().as_str()) {
                    return;
                }
                if self.selected_issue().is_some_and(|i| i.number == number) {
                    self.repo_ctx.issue_body = Some(body);
                }
            }
            DataMsg::RateLimit { remaining, limit } => {
                self.rate_limit = Some((remaining, limit));
                self.rate_limit_updated_at = Some(std::time::Instant::now());
            }
            DataMsg::ViewerPermission { repo, can_push } => {
                if self.current_repo_key().as_deref() == Some(repo.key().as_str()) {
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
                    .insert(owner, (std::time::Instant::now(), prs.clone()));
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
        Some(self.selected_owner_repo()?.to_string())
    }

    /// Clear all state scoped to a single repo. Call this whenever the active repo changes.
    /// Add new per-repo caches here so every transition site is covered automatically.
    pub(crate) fn invalidate_repo(&mut self) {
        self.repo_ctx = RepoCtx::default();
    }

    /// Clear all state scoped to a single source. Calls `invalidate_repo()`, then clears
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

    pub(crate) fn sort_repos_in_place(&mut self) {
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
                            || pr
                                .labels
                                .iter()
                                .any(|l| l.name.to_lowercase().contains(&filter))
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
                    .select(if len > 0 { Some(len - 1) } else { None });
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
                        None | Some(ReviewStatus::Unknown | ReviewStatus::Pending) => 0,
                        Some(ReviewStatus::ChangesRequested) => 1,
                        Some(ReviewStatus::Approved) => 2,
                    }
                };
                self.repo_ctx.prs.sort_by_key(|pr| review_priority(pr));
            }
        }
    }
}
