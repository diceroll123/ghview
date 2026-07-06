use super::super::App;
use crate::{
    actions,
    config::SourcesConfig,
    data::{
        fetch_check_runs, fetch_diff, fetch_issue_body, fetch_issues, fetch_pr_body, fetch_prs,
        fetch_rate_limit, fetch_repo_frontpage, fetch_repos, fetch_review_status,
        fetch_source_issues, fetch_source_prs, fetch_sources, fetch_viewer_approved,
        fetch_viewer_permission, rerun_check,
    },
    types::{
        Column, DataMsg, DetailSection, LoadingKind, PR, PrAction, PrState, RepoId, RepoView,
        ReposView, Source,
    },
};
use ratatui::widgets::ListState;

impl App {
    pub fn trigger_fetch_rate_limit(&self) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if let Ok((remaining, limit)) = fetch_rate_limit().await {
                let _ = tx.send(DataMsg::RateLimit { remaining, limit });
            }
        });
    }

    pub fn trigger_load_sources(&mut self) {
        self.loading = Some(LoadingKind::Sources);
        let tx = self.tx.clone();
        let cfg_sources = SourcesConfig {
            auto_fetch_orgs: self.config.sources.auto_fetch_orgs,
            include_self: self.config.sources.include_self,
            orgs: self.config.sources.orgs.clone(),
            users: self.config.sources.users.clone(),
        };
        tokio::spawn(async move {
            match fetch_sources(&cfg_sources).await {
                Ok((sources, current_user)) => {
                    let _ = tx.send(DataMsg::Sources {
                        sources,
                        current_user,
                    });
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn trigger_load_repos(&mut self) {
        let Some(source) = self.selected_source().cloned() else {
            return;
        };
        let owner = source.owner().to_string();
        let per_page = self.per_page();

        if let Some((fetched_at, cached)) = self
            .repo_cache
            .get(&(owner.clone(), self.repo_sort_key))
            .cloned()
        {
            if fetched_at.elapsed() < self.config.cache_ttl() {
                self.source_ctx
                    .repos_pagination
                    .reset(cached.len() == per_page as usize);
                self.apply_repos(cached);
                if self.source_ctx.repo_state.selected().is_some() {
                    self.trigger_load_prs();
                } else {
                    self.loading = None;
                }
                return;
            }
            self.apply_repos(cached);
        }

        let current_user = self.current_user.clone().unwrap_or_default();
        let sort_key = self.repo_sort_key;
        self.loading = Some(LoadingKind::Repos);
        self.source_ctx.repos_pagination.fetching_more = false;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_repos(&source, &current_user, per_page, 1, sort_key).await {
                Ok(repos) => {
                    let has_more = repos.len() == per_page as usize;
                    let _ = tx.send(DataMsg::Repos {
                        owner,
                        repos,
                        has_more,
                    });
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn force_load_repos(&mut self) {
        if let Some(owner) = self.selected_source_owner() {
            self.repo_cache.retain(|(o, _), _| o != &owner);
        }
        self.trigger_load_repos();
    }

    pub(crate) fn force_load_source_prs(&mut self) {
        if let Some(owner) = self.selected_source_owner() {
            self.source_prs_cache.remove(&owner);
        }
        self.trigger_load_source_prs();
    }

    pub(crate) fn force_load_source_issues(&mut self) {
        if let Some(owner) = self.selected_source_owner() {
            self.source_issues_cache.remove(&owner);
        }
        self.trigger_load_source_issues();
    }

    pub(crate) fn trigger_load_more_repos(&mut self) {
        if !self.source_ctx.repos_pagination.can_load_more() {
            return;
        }
        let Some(source) = self.selected_source().cloned() else {
            return;
        };
        let owner = source.owner().to_string();
        let current_user = self.current_user.clone().unwrap_or_default();
        let per_page = self.per_page();
        let sort_key = self.repo_sort_key;
        self.loading = Some(LoadingKind::Repos);
        let page = self.source_ctx.repos_pagination.begin_fetch();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_repos(&source, &current_user, per_page, page, sort_key).await {
                Ok(repos) => {
                    let has_more = repos.len() == per_page as usize;
                    let _ = tx.send(DataMsg::MoreRepos {
                        owner,
                        repos,
                        has_more,
                    });
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn trigger_load_source_prs(&mut self) {
        let Some(source) = self.selected_source().cloned() else {
            return;
        };
        let owner = source.owner().to_string();
        let is_org = matches!(source, Source::Org(_));
        let per_page = self.per_page();

        if let Some((fetched_at, cached)) = self.source_prs_cache.get(&owner).cloned()
            && fetched_at.elapsed() < self.config.cache_ttl()
        {
            self.source_ctx
                .source_prs_pagination
                .reset(cached.len() == per_page as usize);
            self.apply_source_prs(cached);
            self.loading = None;
            return;
        }

        self.loading = Some(LoadingKind::Prs);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_source_prs(&owner, is_org, per_page, 1).await {
                Ok(prs) => {
                    let has_more = prs.len() == per_page as usize;
                    let _ = tx.send(DataMsg::SourcePrs {
                        owner,
                        prs,
                        has_more,
                    });
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn trigger_load_more_source_prs(&mut self) {
        if !self.source_ctx.source_prs_pagination.can_load_more() {
            return;
        }
        let Some(source) = self.selected_source().cloned() else {
            return;
        };
        let owner = source.owner().to_string();
        let is_org = matches!(source, Source::Org(_));
        let per_page = self.per_page();
        let page = self.source_ctx.source_prs_pagination.begin_fetch();
        self.loading = Some(LoadingKind::Prs);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_source_prs(&owner, is_org, per_page, page).await {
                Ok(prs) => {
                    let has_more = prs.len() == per_page as usize;
                    let _ = tx.send(DataMsg::MoreSourcePrs {
                        owner,
                        prs,
                        has_more,
                    });
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn trigger_load_source_issues(&mut self) {
        let Some(source) = self.selected_source().cloned() else {
            return;
        };
        let owner = source.owner().to_string();
        let is_org = matches!(source, Source::Org(_));
        let per_page = self.per_page();

        if let Some((fetched_at, cached)) = self.source_issues_cache.get(&owner).cloned()
            && fetched_at.elapsed() < self.config.cache_ttl()
        {
            self.source_ctx
                .source_issues_pagination
                .reset(cached.len() == per_page as usize);
            self.apply_source_issues(cached);
            self.loading = None;
            return;
        }

        self.loading = Some(LoadingKind::Issues);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_source_issues(&owner, is_org, per_page, 1).await {
                Ok(issues) => {
                    let has_more = issues.len() == per_page as usize;
                    let _ = tx.send(DataMsg::SourceIssues {
                        owner,
                        issues,
                        has_more,
                    });
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn trigger_load_more_source_issues(&mut self) {
        if !self.source_ctx.source_issues_pagination.can_load_more() {
            return;
        }
        let Some(source) = self.selected_source().cloned() else {
            return;
        };
        let owner = source.owner().to_string();
        let is_org = matches!(source, Source::Org(_));
        let per_page = self.per_page();
        let page = self.source_ctx.source_issues_pagination.begin_fetch();
        self.loading = Some(LoadingKind::Issues);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_source_issues(&owner, is_org, per_page, page).await {
                Ok(issues) => {
                    let has_more = issues.len() == per_page as usize;
                    let _ = tx.send(DataMsg::MoreSourceIssues {
                        owner,
                        issues,
                        has_more,
                    });
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn trigger_load_source_issue_body(&mut self) {
        let Some(issue) = self.selected_source_issue().cloned() else {
            return;
        };
        let owner = self.selected_source_owner().unwrap_or_default();
        let actual_owner = if issue.repo_owner.is_empty() {
            owner
        } else {
            issue.repo_owner.clone()
        };
        let rid = RepoId::new(actual_owner, issue.repo.clone());
        let number = issue.number;
        self.repo_ctx.issue_body = None;
        self.repo_ctx.issue_body_scroll = 0;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if let Ok(body) = fetch_issue_body(&rid, number).await {
                let _ = tx.send(DataMsg::IssueBody {
                    repo: rid,
                    number,
                    body,
                });
            }
        });
    }

    pub(crate) fn trigger_load_pr_body(&mut self) {
        let Some((rid, pr)) = self.selected_pr_context() else {
            return;
        };
        let pr_number = pr.number;
        let known_sha = pr.head_sha.clone();
        self.repo_ctx.pr_body = None;
        self.repo_ctx.check_runs = None;
        self.repo_ctx.check_runs_state = ListState::default();
        self.repo_ctx.pr_body_scroll = 0;
        self.repo_ctx.detail_section = DetailSection::default();
        self.repo_ctx.diff_view = None;
        let pr_id = rid.pr(pr_number);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let (body, mergeable_state, additions, deletions, sha, auto_merge) =
                fetch_pr_body(&pr_id.repo, pr_number)
                    .await
                    .unwrap_or_default();
            let _ = tx.send(DataMsg::PrBody {
                pr: pr_id.clone(),
                body,
                mergeable_state,
                additions,
                deletions,
                auto_merge,
            });
            // Use the SHA from the API response; fall back to the value already in the PR
            // struct (populated for repo-list PRs but empty for source-list PRs).
            let sha = if sha.is_empty() { known_sha } else { sha };
            if !sha.is_empty() {
                let runs = fetch_check_runs(&pr_id.repo, &sha).await;
                let _ = tx.send(DataMsg::CheckRuns { pr: pr_id, runs });
            }
        });
    }

    pub(crate) fn trigger_load_prs(&mut self) {
        let Some(rid) = self.selected_owner_repo() else {
            return;
        };
        if !self.selected_repo_has_prs() {
            self.loading = None;
            return;
        }
        self.invalidate_repo();
        let key = rid.key();
        {
            let needs_fetch = match self.permission_cache.get(&key).copied() {
                Some((fetched_at, (can_push, allow_auto_merge))) => {
                    self.repo_ctx.viewer_can_push = Some(can_push);
                    self.repo_ctx.allow_auto_merge = Some(allow_auto_merge);
                    fetched_at.elapsed() >= self.config.cache_ttl()
                }
                None => true,
            };
            if needs_fetch {
                let repo_id = rid.clone();
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let (can_push, allow_auto_merge) = fetch_viewer_permission(&repo_id).await;
                    let _ = tx.send(DataMsg::ViewerPermission {
                        repo: repo_id,
                        can_push,
                        allow_auto_merge,
                    });
                });
            }
        }

        if let Some((fetched_at, cached)) = self.pr_cache.get(&key).cloned() {
            if fetched_at.elapsed() < self.config.cache_ttl() {
                self.apply_prs(cached);
                self.loading = None;
                return;
            }
            // Stale cache: show existing data, refresh silently in background.
            self.apply_prs(cached);
            self.loading = None;
            let per_page = self.per_page();
            let tx = self.tx.clone();
            let rid2 = rid;
            tokio::spawn(async move {
                match fetch_prs(&rid2, per_page, 1).await {
                    Ok(prs) => {
                        let has_more = prs.len() == per_page as usize;
                        let _ = tx.send(DataMsg::Prs {
                            repo: rid2,
                            prs,
                            has_more,
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(DataMsg::Error(e.to_string()));
                    }
                }
            });
            return;
        }

        if self.repo_view == crate::types::RepoView::Prs {
            self.loading = Some(LoadingKind::Prs);
        }
        self.repo_ctx.prs_pagination.fetching_more = false;
        let per_page = self.per_page();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_prs(&rid, per_page, 1).await {
                Ok(prs) => {
                    let has_more = prs.len() == per_page as usize;
                    let _ = tx.send(DataMsg::Prs {
                        repo: rid,
                        prs,
                        has_more,
                    });
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn trigger_load_more_prs(&mut self) {
        if !self.repo_ctx.prs_pagination.can_load_more() {
            return;
        }
        if !self.selected_repo_has_prs() {
            return;
        }
        let Some(rid) = self.selected_owner_repo() else {
            return;
        };
        let per_page = self.per_page();
        let page = self.repo_ctx.prs_pagination.begin_fetch();
        self.loading = Some(LoadingKind::Prs);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_prs(&rid, per_page, page).await {
                Ok(prs) => {
                    let has_more = prs.len() == per_page as usize;
                    let _ = tx.send(DataMsg::MorePrs {
                        repo: rid,
                        prs,
                        has_more,
                    });
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn trigger_review_and_check_fetches(&self) {
        if self.repos_view == ReposView::PrList {
            self.trigger_source_pr_review_fetches();
            return;
        }
        let Some(rid) = self.selected_owner_repo() else {
            return;
        };
        let key = rid.key();
        let tx = self.tx.clone();

        let existing_reviews = self.review_cache.get(&key).cloned().unwrap_or_default();
        let prs_to_fetch: Vec<PR> = self
            .repo_ctx
            .prs
            .iter()
            .filter(|pr| !existing_reviews.contains_key(&pr.number))
            .cloned()
            .collect();

        if prs_to_fetch.is_empty() {
            return;
        }

        let RepoId { owner, repo } = rid;
        let owner: std::sync::Arc<str> = owner.into();
        let repo: std::sync::Arc<str> = repo.into();

        for pr in prs_to_fetch {
            let rid = RepoId::new(owner.as_ref(), repo.as_ref());
            let tx2 = tx.clone();
            let num = pr.number;
            tokio::spawn(async move {
                let status = fetch_review_status(&rid, num).await;
                let _ = tx2.send(DataMsg::ReviewStatus {
                    pr: rid.pr(num),
                    status,
                });
            });
        }
    }

    fn trigger_source_pr_review_fetches(&self) {
        let Some(source_owner) = self.selected_source_owner() else {
            return;
        };
        for pr in &self.source_ctx.source_prs {
            let actual_owner = if pr.repo_owner.is_empty() {
                source_owner.clone()
            } else {
                pr.repo_owner.clone()
            };
            let key = format!("{actual_owner}/{}", pr.repo);
            if self
                .review_cache
                .get(&key)
                .is_some_and(|m| m.contains_key(&pr.number))
            {
                continue;
            }
            let rid = RepoId::new(actual_owner, pr.repo.clone());
            let num = pr.number;
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let status = fetch_review_status(&rid, num).await;
                let _ = tx.send(DataMsg::ReviewStatus {
                    pr: rid.pr(num),
                    status,
                });
            });
        }
    }

    pub(crate) fn trigger_prefetch_pr_details(&mut self) {
        if !self.config.ui.prefetch_pr_details {
            return;
        }

        // Cap concurrent gh subprocesses to avoid overwhelming the system (especially in
        // containers where running dozens of gh processes at once causes silent failures).
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(5));

        if self.repos_view == ReposView::PrList {
            let Some(source_owner) = self.selected_source_owner() else {
                return;
            };
            for pr in &self.source_ctx.source_prs {
                let actual_owner = if pr.repo_owner.is_empty() {
                    source_owner.clone()
                } else {
                    pr.repo_owner.clone()
                };
                let pr_id = RepoId::new(actual_owner, pr.repo.clone()).pr(pr.number);
                if self.repo_ctx.mergeable_states.contains_key(&pr_id) {
                    continue;
                }
                let tx = self.tx.clone();
                let sem = sem.clone();
                let pr_number = pr_id.number;
                tokio::spawn(async move {
                    let _permit = sem.acquire_owned().await.unwrap();
                    let (body, mergeable_state, additions, deletions, sha, auto_merge) =
                        fetch_pr_body(&pr_id.repo, pr_number)
                            .await
                            .unwrap_or_default();
                    let _ = tx.send(DataMsg::PrBody {
                        pr: pr_id.repo.clone().pr(pr_number),
                        body,
                        mergeable_state,
                        additions,
                        deletions,
                        auto_merge,
                    });
                    if !sha.is_empty() {
                        let runs = fetch_check_runs(&pr_id.repo, &sha).await;
                        let _ = tx.send(DataMsg::CheckRuns {
                            pr: pr_id.repo.pr(pr_number),
                            runs,
                        });
                    }
                });
            }
            return;
        }

        let Some(rid) = self.selected_owner_repo() else {
            return;
        };

        for pr in &self.repo_ctx.prs {
            let id = rid.clone().pr(pr.number);

            // The list API already returns head_sha, so fetch check runs immediately without
            // waiting for the body fetch to complete.
            if !pr.head_sha.is_empty() && !self.repo_ctx.check_summary_cache.contains_key(&id) {
                let tx = self.tx.clone();
                let pr_id = id.clone();
                let sha = pr.head_sha.clone();
                let sem = sem.clone();
                tokio::spawn(async move {
                    let _permit = sem.acquire_owned().await.unwrap();
                    let runs = fetch_check_runs(&pr_id.repo, &sha).await;
                    let _ = tx.send(DataMsg::CheckRuns { pr: pr_id, runs });
                });
            }

            // Fetch body, diff stats, and mergeable state.
            if !self.repo_ctx.mergeable_states.contains_key(&id) {
                let tx = self.tx.clone();
                let pr_id = id;
                let pr_number = pr.number;
                let sem = sem.clone();
                tokio::spawn(async move {
                    let _permit = sem.acquire_owned().await.unwrap();
                    let (body, mergeable_state, additions, deletions, _, auto_merge) =
                        fetch_pr_body(&pr_id.repo, pr_number)
                            .await
                            .unwrap_or_default();
                    let _ = tx.send(DataMsg::PrBody {
                        pr: pr_id.repo.pr(pr_number),
                        body,
                        mergeable_state,
                        additions,
                        deletions,
                        auto_merge,
                    });
                });
            }
        }
    }

    pub(crate) fn trigger_load_frontpage(&mut self) {
        let Some(rid) = self.selected_owner_repo() else {
            return;
        };
        let key = rid.key();

        if let Some((fetched_at, cached)) = self.frontpage_cache.get(&key).cloned() {
            if fetched_at.elapsed() < self.config.cache_ttl() {
                self.repo_ctx.repo_frontpage = Some(cached);
                self.loading = None;
                return;
            }
            // Stale: show cached while refreshing silently in background.
            self.repo_ctx.repo_frontpage = Some(cached);
            self.loading = None;
            let tx = self.tx.clone();
            tokio::spawn(async move {
                if let Ok((description, readme)) = fetch_repo_frontpage(&rid).await {
                    let _ = tx.send(DataMsg::RepoFrontpage {
                        repo: rid,
                        description,
                        readme,
                    });
                }
            });
            return;
        }

        self.repo_ctx.repo_frontpage = None;
        self.repo_ctx.repo_frontpage_scroll = 0;
        self.loading = Some(LoadingKind::Frontpage);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if let Ok((description, readme)) = fetch_repo_frontpage(&rid).await {
                let _ = tx.send(DataMsg::RepoFrontpage {
                    repo: rid,
                    description,
                    readme,
                });
            }
        });
    }

    pub(crate) fn trigger_load_issues(&mut self) {
        let Some(rid) = self.selected_owner_repo() else {
            return;
        };
        self.repo_ctx.issues = vec![];
        self.repo_ctx.issue_state = ListState::default();
        self.repo_ctx.issue_body = None;
        self.repo_ctx.issue_body_scroll = 0;
        self.loading = Some(LoadingKind::Issues);
        self.repo_ctx.issues_pagination.fetching_more = false;
        let per_page = self.per_page();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_issues(&rid, per_page, 1).await {
                Ok((issues, has_more)) => {
                    let _ = tx.send(DataMsg::Issues {
                        repo: rid,
                        issues,
                        has_more,
                    });
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn trigger_load_more_issues(&mut self) {
        if !self.repo_ctx.issues_pagination.can_load_more() {
            return;
        }
        let Some(rid) = self.selected_owner_repo() else {
            return;
        };
        let per_page = self.per_page();
        let page = self.repo_ctx.issues_pagination.begin_fetch();
        self.loading = Some(LoadingKind::Issues);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_issues(&rid, per_page, page).await {
                Ok((issues, has_more)) => {
                    let _ = tx.send(DataMsg::MoreIssues {
                        repo: rid,
                        issues,
                        has_more,
                    });
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn trigger_load_issue_body(&mut self) {
        let Some((rid, issue)) = self.selected_issue_context() else {
            return;
        };
        let number = issue.number;
        self.repo_ctx.issue_body = None;
        self.repo_ctx.issue_body_scroll = 0;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if let Ok(body) = fetch_issue_body(&rid, number).await {
                let _ = tx.send(DataMsg::IssueBody {
                    repo: rid,
                    number,
                    body,
                });
            }
        });
    }

    pub(crate) fn dispatch_repo_view_trigger(&mut self) {
        match self.repo_view {
            RepoView::Frontpage => self.trigger_load_frontpage(),
            RepoView::Prs => {
                if self.repo_ctx.prs.is_empty() {
                    self.trigger_load_prs();
                }
                self.trigger_load_pr_body();
            }
            RepoView::Issues => self.trigger_load_issues(),
        }
    }

    pub(crate) fn switch_repo_view(&mut self, view: RepoView) {
        if self.repo_view == view {
            return;
        }
        self.repo_view = view;
        self.focus = Column::Repo;
        self.repo_ctx.pr_body_scroll = 0;
        self.repo_ctx.issue_body_scroll = 0;
        self.repo_ctx.repo_frontpage_scroll = 0;
        self.dispatch_repo_view_trigger();
    }

    pub(crate) fn try_switch_repo_view(&mut self, view: RepoView) {
        let blocked = match view {
            RepoView::Prs => (!self.selected_repo_has_prs())
                .then_some("Pull requests are disabled for this repository"),
            RepoView::Issues => (!self.selected_repo_has_issues())
                .then_some("Issues are disabled for this repository"),
            RepoView::Frontpage => None,
        };
        if let Some(msg) = blocked {
            self.set_status(msg.to_string());
        } else {
            self.switch_repo_view(view);
        }
    }

    pub(crate) fn trigger_refresh(&mut self) {
        match self.focus {
            Column::Sources => self.trigger_load_sources(),
            Column::Repos => match self.repos_view {
                ReposView::PrList => self.force_load_source_prs(),
                ReposView::IssueList => self.force_load_source_issues(),
                ReposView::RepoList => self.force_load_repos(),
            },
            Column::Repo | Column::Detail => match self.repo_view {
                RepoView::Frontpage => {
                    if let Some(rid) = self.selected_owner_repo() {
                        self.frontpage_cache.remove(&rid.key());
                    }
                    self.trigger_load_frontpage();
                }
                RepoView::Prs => self.force_load_prs(),
                RepoView::Issues => self.trigger_load_issues(),
            },
        }
    }

    pub(crate) fn force_load_prs(&mut self) {
        if let Some(key) = self.current_repo_key() {
            self.pr_cache.remove(&key);
            self.review_cache.remove(&key);
        }
        self.trigger_load_prs();
    }

    pub(crate) fn trigger_load_diff(&mut self) {
        let Some((rid, pr)) = self.selected_pr_context() else {
            return;
        };
        let title = format!("#{} {}", pr.number, pr.title);
        self.loading = Some(LoadingKind::Action("diff".into()));
        self.repo_ctx.diff_view = None;
        let tx = self.tx.clone();
        let pr_number = pr.number;
        tokio::spawn(async move {
            match fetch_diff(&rid, pr_number).await {
                Ok(content) => {
                    let _ = tx.send(DataMsg::DiffContent {
                        pr: rid.pr(pr_number),
                        title,
                        content,
                    });
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn do_pr_action(&mut self, action: PrAction) {
        let Some(pr_id) = self.selected_pr_id() else {
            return;
        };

        // In-flight guard
        if self.loading.is_some() {
            return;
        }

        // Fast-path local guards for actions where PR list state is authoritative
        match action {
            PrAction::Close => {
                if self
                    .selected_pr()
                    .is_some_and(|p| p.state == PrState::Closed)
                {
                    self.set_status(format!("Already closed #{}", pr_id.number));
                    return;
                }
            }
            PrAction::Reopen => {
                if self
                    .selected_pr()
                    .is_some_and(|p| p.state != PrState::Closed)
                {
                    self.set_status(format!("Already open #{}", pr_id.number));
                    return;
                }
            }
            PrAction::MarkReady => {
                if self.selected_pr().is_some_and(|p| !p.draft) {
                    self.set_status(format!("Already ready for review #{}", pr_id.number));
                    return;
                }
            }
            PrAction::Merge => {
                let use_auto = self.merge_uses_auto();
                if use_auto && self.selected_pr().is_some_and(|p| p.auto_merge) {
                    self.set_status(format!("Auto-merge already enabled #{}", pr_id.number));
                    return;
                }
            }
            PrAction::Approve => {}
        }

        let tx = self.tx.clone();
        let use_auto = self.merge_uses_auto();
        let current_user = self.current_user.clone();
        let action_label = if action == PrAction::Merge && !use_auto {
            "merge"
        } else {
            action.label()
        };
        self.loading = Some(LoadingKind::Action(action_label.into()));

        let merge_method = self.config.ui.merge_method;
        tokio::spawn(async move {
            // Live API pre-check: skip approve if the viewer already approved
            let pr_number = pr_id.number;
            if action == PrAction::Approve
                && let Some(ref user) = current_user
                && fetch_viewer_approved(&pr_id.repo, pr_number, user).await
            {
                let _ = tx.send(DataMsg::PrActionDone {
                    pr: pr_id,
                    action,
                    use_auto,
                    msg: Some(format!("Already approved #{pr_number}")),
                });
                return;
            }

            let result = match action {
                PrAction::Approve => actions::approve(&pr_id).await,
                PrAction::Merge => actions::merge(&pr_id, merge_method, use_auto).await,
                PrAction::Close => actions::close_pr(&pr_id).await,
                PrAction::Reopen => actions::reopen_pr(&pr_id).await,
                PrAction::MarkReady => actions::mark_ready(&pr_id).await,
            }
            .map(|()| {
                let msg = if action == PrAction::Merge && use_auto {
                    format!("Auto-merge enabled #{}", pr_id.number)
                } else {
                    action.success_msg(pr_id.number)
                };
                Some(msg)
            });
            match result {
                Ok(msg) => {
                    let _ = tx.send(DataMsg::PrActionDone {
                        pr: pr_id,
                        action,
                        use_auto,
                        msg,
                    });
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn diff_scroll(&mut self, n: u16) {
        if let Some(d) = &mut self.repo_ctx.diff_view {
            let max = u16::try_from(d.lines.len().saturating_sub(1)).unwrap_or(u16::MAX);
            d.scroll = (d.scroll + n).min(max);
        }
    }

    pub(crate) fn diff_scroll_up(&mut self, n: u16) {
        if let Some(d) = &mut self.repo_ctx.diff_view {
            d.scroll = d.scroll.saturating_sub(n);
        }
    }

    pub(crate) fn rerun_selected_check(&mut self) {
        let Some(idx) = self.repo_ctx.check_runs_state.selected() else {
            return;
        };
        let Some(runs) = &self.repo_ctx.check_runs else {
            return;
        };
        let Some(run) = runs.get(idx) else { return };
        let check_run_id = run.id;
        let name = run.name.clone();
        let Some(rid) = self.selected_owner_repo() else {
            return;
        };
        self.set_status(format!("Re-running {name}\u{2026}"));
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match rerun_check(&rid, check_run_id).await {
                Ok(()) => {
                    let _ = tx.send(DataMsg::ActionDone(Some(format!("Re-running {name}"))));
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }
}
