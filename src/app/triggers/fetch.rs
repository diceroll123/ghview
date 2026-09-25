use super::super::App;
use crate::{
    actions,
    config::SourcesConfig,
    data::{
        fetch_check_runs, fetch_diff, fetch_issue_body, fetch_issues, fetch_pr_body,
        fetch_pr_comments, fetch_pr_commits, fetch_pr_files, fetch_prs, fetch_rate_limit,
        fetch_repo_frontpage, fetch_repos, fetch_review_status, fetch_source_issues,
        fetch_source_prs, fetch_sources, fetch_viewer_permission, rerun_check,
    },
    types::{
        Column, DataMsg, DetailSection, LoadKey, PR, PrAction, PrId, PrState, RepoId, RepoView,
        ReposView, Source,
    },
};
use ratatui::widgets::ListState;

impl App {
    fn spawn_page_fetch<T>(
        &self,
        per_page: u32,
        fetch: impl std::future::Future<Output = anyhow::Result<Vec<T>>> + Send + 'static,
        on_msg: impl FnOnce(Vec<T>, bool) -> DataMsg + Send + 'static,
    ) where
        T: Send + 'static,
    {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch.await {
                Ok(items) => {
                    let has_more = items.len() == per_page as usize;
                    let _ = tx.send(on_msg(items, has_more));
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub fn trigger_fetch_rate_limit(&self) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if let Ok((remaining, limit)) = fetch_rate_limit().await {
                let _ = tx.send(DataMsg::RateLimit { remaining, limit });
            }
        });
    }

    pub fn trigger_load_sources(&mut self) {
        self.set_loading(LoadKey::Sources);
        let tx = self.tx.clone();
        let cfg_sources = SourcesConfig {
            auto_fetch_orgs: self.config.sources.auto_fetch_orgs,
            include_self: self.config.sources.include_self,
            orgs: self.config.sources.orgs.clone(),
            users: self.config.sources.users.clone(),
            exclude: self.config.sources.exclude.clone(),
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
            // Nothing to fetch (e.g. a filter hid every source): drop the key so an
            // in-flight spinner from a previous selection doesn't stick.
            self.clear_loading(&LoadKey::Repos);
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
                // The repos pane is done as soon as its data lands - whether from this
                // fetch or the cache. Clearing only in the no-selection branch leaked the
                // key when a source switch raced an in-flight fetch for another source:
                // the stale message is discarded by its owner guard, and nothing else
                // ever cleared the key, so "loading repos…" stuck forever.
                self.clear_loading(&LoadKey::Repos);
                self.source_ctx
                    .repos_pagination
                    .reset(cached.len() == per_page as usize);
                self.apply_repos(cached);
                if self.source_ctx.repo_state.selected().is_some() {
                    self.trigger_load_prs();
                }
                return;
            }
            self.apply_repos(cached);
        }

        let current_user = self.current_user.clone().unwrap_or_default();
        let sort_key = self.repo_sort_key;
        self.set_loading(LoadKey::Repos);
        self.source_ctx.repos_pagination.fetching_more = false;
        self.spawn_page_fetch(
            per_page,
            async move { fetch_repos(&source, &current_user, per_page, 1, sort_key).await },
            move |repos, has_more| DataMsg::Repos {
                owner,
                repos,
                has_more,
            },
        );
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
        // Hard refresh: the list is replaced wholesale, so any multi-selection is stale.
        self.clear_pr_selection();
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
        self.set_loading(LoadKey::Repos);
        let page = self.source_ctx.repos_pagination.begin_fetch();
        self.spawn_page_fetch(
            per_page,
            async move { fetch_repos(&source, &current_user, per_page, page, sort_key).await },
            move |repos, has_more| DataMsg::MoreRepos {
                owner,
                repos,
                has_more,
            },
        );
    }

    pub(crate) fn trigger_load_source_prs(&mut self) {
        let Some(source) = self.selected_source().cloned() else {
            // No source is selectable (e.g. a filter hid every entry while a fetch was in
            // flight): nothing will ever clear the key, so drop it here.
            self.clear_loading(&LoadKey::SourcePrs);
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
            self.clear_loading(&LoadKey::SourcePrs);
            return;
        }

        self.set_loading_for(LoadKey::SourcePrs, owner.clone());
        let owner_msg = owner.clone();
        self.spawn_page_fetch(
            per_page,
            async move { fetch_source_prs(&owner, is_org, per_page, 1).await },
            move |prs, has_more| DataMsg::SourcePrs {
                owner: owner_msg,
                prs,
                has_more,
            },
        );
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
        self.set_loading_for(LoadKey::SourcePrs, owner.clone());
        let owner_msg = owner.clone();
        self.spawn_page_fetch(
            per_page,
            async move { fetch_source_prs(&owner, is_org, per_page, page).await },
            move |prs, has_more| DataMsg::MoreSourcePrs {
                owner: owner_msg,
                prs,
                has_more,
            },
        );
    }

    pub(crate) fn trigger_load_source_issues(&mut self) {
        let Some(source) = self.selected_source().cloned() else {
            // No source is selectable (e.g. a filter hid every entry while a fetch was in
            // flight): nothing will ever clear the key, so drop it here.
            self.clear_loading(&LoadKey::SourceIssues);
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
            self.clear_loading(&LoadKey::SourceIssues);
            return;
        }

        self.set_loading_for(LoadKey::SourceIssues, owner.clone());
        let owner_msg = owner.clone();
        self.spawn_page_fetch(
            per_page,
            async move { fetch_source_issues(&owner, is_org, per_page, 1).await },
            move |issues, has_more| DataMsg::SourceIssues {
                owner: owner_msg,
                issues,
                has_more,
            },
        );
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
        self.set_loading_for(LoadKey::SourceIssues, owner.clone());
        let owner_msg = owner.clone();
        self.spawn_page_fetch(
            per_page,
            async move { fetch_source_issues(&owner, is_org, per_page, page).await },
            move |issues, has_more| DataMsg::MoreSourceIssues {
                owner: owner_msg,
                issues,
                has_more,
            },
        );
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
        // A diff for the previous PR (if any) is now stale: its in-flight message will be
        // discarded by the DiffContent guard, so drop its spinner here or it sticks.
        self.clear_action_named("diff");
        self.repo_ctx.pr_activity = None;
        self.repo_ctx.pr_activity_scroll = 0;
        self.repo_ctx.pr_commits = None;
        self.repo_ctx.pr_commits_state = ListState::default();
        self.repo_ctx.pr_files = None;
        self.repo_ctx.pr_files_state = ListState::default();
        let pr_id = rid.clone().pr(pr_number);
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

        let pr_id = rid.clone().pr(pr_number);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let comments = fetch_pr_comments(&pr_id.repo, pr_number).await;
            let _ = tx.send(DataMsg::PrActivity {
                pr: pr_id,
                comments,
            });
        });

        let pr_id = rid.pr(pr_number);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let commits = fetch_pr_commits(&pr_id.repo, pr_number).await;
            let _ = tx.send(DataMsg::PrCommits {
                pr: pr_id.clone(),
                commits,
            });
            let files = fetch_pr_files(&pr_id.repo, pr_number).await;
            let _ = tx.send(DataMsg::PrFiles { pr: pr_id, files });
        });
    }

    pub(crate) fn trigger_load_prs(&mut self) {
        let Some(rid) = self.selected_owner_repo() else {
            // No repo is selectable (e.g. the user moved to a source with no repos while a
            // PR fetch was in flight): nothing will ever clear the key, so drop it here.
            self.clear_loading(&LoadKey::RepoPrs);
            return;
        };
        if !self.selected_repo_has_prs() {
            self.clear_loading(&LoadKey::RepoPrs);
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
                self.clear_loading(&LoadKey::RepoPrs);
                return;
            }
            // Stale cache: show existing data, refresh silently in background.
            self.apply_prs(cached);
            self.clear_loading(&LoadKey::RepoPrs);
            let per_page = self.per_page();
            let rid2 = rid;
            let rid_msg = rid2.clone();
            self.spawn_page_fetch(
                per_page,
                async move { fetch_prs(&rid2, per_page, 1).await },
                move |prs, has_more| DataMsg::Prs {
                    repo: rid_msg,
                    prs,
                    has_more,
                },
            );
            return;
        }

        if self.repo_view == crate::types::RepoView::Prs {
            self.set_loading_for(LoadKey::RepoPrs, rid.key());
        }
        self.repo_ctx.prs_pagination.fetching_more = false;
        let per_page = self.per_page();
        let rid_msg = rid.clone();
        self.spawn_page_fetch(
            per_page,
            async move { fetch_prs(&rid, per_page, 1).await },
            move |prs, has_more| DataMsg::Prs {
                repo: rid_msg,
                prs,
                has_more,
            },
        );
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
        self.set_loading_for(LoadKey::RepoPrs, rid.key());
        let rid_msg = rid.clone();
        self.spawn_page_fetch(
            per_page,
            async move { fetch_prs(&rid, per_page, page).await },
            move |prs, has_more| DataMsg::MorePrs {
                repo: rid_msg,
                prs,
                has_more,
            },
        );
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

        let current_user = self.current_user.clone();
        let RepoId { owner, repo } = rid;
        let owner: std::sync::Arc<str> = owner.into();
        let repo: std::sync::Arc<str> = repo.into();

        for pr in prs_to_fetch {
            let rid = RepoId::new(owner.as_ref(), repo.as_ref());
            let tx2 = tx.clone();
            let num = pr.number;
            let current_user = current_user.clone();
            tokio::spawn(async move {
                let (status, viewer_approved) =
                    fetch_review_status(&rid, num, current_user.as_deref()).await;
                let _ = tx2.send(DataMsg::ReviewStatus {
                    pr: rid.pr(num),
                    status,
                    viewer_approved,
                });
            });
        }
    }

    fn trigger_source_pr_review_fetches(&self) {
        let Some(source_owner) = self.selected_source_owner() else {
            return;
        };
        let current_user = self.current_user.clone();
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
            let current_user = current_user.clone();
            tokio::spawn(async move {
                let (status, viewer_approved) =
                    fetch_review_status(&rid, num, current_user.as_deref()).await;
                let _ = tx.send(DataMsg::ReviewStatus {
                    pr: rid.pr(num),
                    status,
                    viewer_approved,
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
            // No repo is selectable (e.g. the user moved to a source with no repos while a
            // frontpage fetch was in flight): nothing will ever clear the key, so drop it.
            self.clear_loading(&LoadKey::Frontpage);
            return;
        };
        let key = rid.key();

        if let Some((fetched_at, cached)) = self.frontpage_cache.get(&key).cloned() {
            if fetched_at.elapsed() < self.config.cache_ttl() {
                self.repo_ctx.repo_frontpage = Some(cached);
                self.clear_loading(&LoadKey::Frontpage);
                return;
            }
            // Stale: show cached while refreshing silently in background.
            self.repo_ctx.repo_frontpage = Some(cached);
            self.clear_loading(&LoadKey::Frontpage);
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
        self.set_loading_for(LoadKey::Frontpage, rid.key());
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
            // No repo is selectable (e.g. a filter hid every entry while an issues fetch was
            // in flight): nothing will ever clear the key, so drop it here.
            self.clear_loading(&LoadKey::RepoIssues);
            return;
        };
        self.repo_ctx.issues = vec![];
        self.repo_ctx.issue_state = ListState::default();
        self.repo_ctx.issue_body = None;
        self.repo_ctx.issue_body_scroll = 0;
        self.set_loading_for(LoadKey::RepoIssues, rid.key());
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
        self.set_loading_for(LoadKey::RepoIssues, rid.key());
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
        // Hard refresh: the list is replaced wholesale, so any multi-selection is stale.
        self.clear_pr_selection();
        self.trigger_load_prs();
    }

    pub(crate) fn trigger_load_diff(&mut self) {
        let Some((rid, pr)) = self.selected_pr_context() else {
            return;
        };
        let title = format!("#{} {}", pr.number, pr.title);
        self.set_loading(LoadKey::Action("diff".into()));
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

    /// Run a PR action against the whole selection when it is active, otherwise just the
    /// cursor PR. Each target spawns its own task so a batch runs concurrently; loading
    /// and the pending counter are held until every target reports back. Per-PR fast-path
    /// guards (already closed/open/draft/approved/auto_merge) skip work and, for a
    /// single-PR action, still surface the "already ..." status as before.
    pub(crate) fn do_pr_action_batch(&mut self, action: PrAction) {
        // In-flight guard: don't stack a new batch on top of one already running.
        if self.is_loading() || self.pending_pr_actions > 0 {
            return;
        }

        let visible = self.active_visible_prs();
        let targets: Vec<(PrId, PR)> = if self.pr_selection_active() {
            self.selected_prs
                .iter()
                .filter_map(|id| {
                    visible
                        .iter()
                        .find(|pr| &self.pr_id_of(pr) == id)
                        .map(|pr| (id.clone(), (*pr).clone()))
                })
                .collect()
        } else {
            self.active_pr_id()
                .and_then(|id| {
                    visible
                        .iter()
                        .find(|pr| self.pr_id_of(pr) == id)
                        .map(|pr| (id, (*pr).clone()))
                })
                .into_iter()
                .collect()
        };

        if targets.is_empty() {
            return;
        }

        // Per-PR fast-path: skip targets where the action is already a no-op.
        let actionable: Vec<(PrId, PR)> = targets
            .iter()
            .filter(|t| !self.pr_action_already_done(action, &t.1))
            .cloned()
            .collect();

        if actionable.is_empty() {
            // Single-PR action where the only target was a no-op: keep the old status.
            if targets.len() == 1 {
                self.set_status(self.already_done_msg(action, &targets[0]));
            }
            return;
        }

        let n = actionable.len() as u32;
        self.pending_pr_actions += n;

        let single = n == 1 && targets.len() == 1;
        // A batch (more than one target, or any selection) shows a single summary when
        // it completes; a lone cursor-PR action keeps the per-action status as before.
        let is_batch = !single;
        if is_batch {
            self.batch_total = n;
            self.batch_failed = 0;
            self.batch_summary_ok = Some(action.batch_success_msg(n));
        } else {
            self.batch_total = 0;
            self.batch_failed = 0;
            self.batch_summary_ok = None;
        }

        let action_label = match (action, self.merge_uses_auto_for(&actionable[0].1)) {
            (PrAction::Merge, false) => "merge",
            _ => action.label(),
        };
        let label = if single {
            action_label.to_string()
        } else {
            format!("{action_label} x{n}")
        };

        let tx = self.tx.clone();
        let merge_method = self.config.ui.merge_method;
        self.set_loading(LoadKey::Action(label));

        for (pr_id, pr) in actionable {
            let use_auto = action == PrAction::Merge && self.merge_uses_auto_for(&pr);
            // Merges into the same base branch run one at a time: GitHub rejects a merge
            // whose base moved since state was read, so concurrent same-branch merges are
            // a self-inflicted race. The lock is held for the whole gh call (incl. any
            // retries). Other actions stay fully concurrent.
            let merge_lock = (action == PrAction::Merge).then(|| self.merge_lock_for(&pr));
            let tx = tx.clone();
            tokio::spawn(async move {
                let _merge_guard = match merge_lock.as_ref() {
                    Some(l) => Some(l.lock().await),
                    None => None,
                };
                let result = match action {
                    PrAction::Approve => actions::approve(&pr_id).await,
                    PrAction::Merge => actions::merge(&pr_id, merge_method, use_auto).await,
                    PrAction::Close => actions::close_pr(&pr_id).await,
                    PrAction::Reopen => actions::reopen_pr(&pr_id).await,
                    PrAction::MarkReady => actions::mark_ready(&pr_id).await,
                };
                match result {
                    Ok(()) => {
                        // Single-PR action shows its own status; a batch suppresses it in
                        // favor of one summary shown when the counter reaches zero.
                        let msg = single.then(|| {
                            if action == PrAction::Merge && use_auto {
                                format!("Auto-merge enabled #{}", pr_id.number)
                            } else {
                                action.success_msg(pr_id.number)
                            }
                        });
                        let _ = tx.send(DataMsg::PrActionDone {
                            pr: pr_id,
                            action,
                            use_auto,
                            msg,
                        });
                    }
                    Err(e) => {
                        // Always route through PrActionError so the pending counter is
                        // decremented (the generic Error arm does not touch it). Single
                        // actions show the message directly; batches count and summarize.
                        let _ = tx.send(DataMsg::PrActionError {
                            pr: pr_id,
                            msg: e.to_string(),
                        });
                    }
                }
            });
        }
    }

    /// True when `action` is already a no-op for this PR (list state is authoritative).
    fn pr_action_already_done(&self, action: PrAction, pr: &PR) -> bool {
        match action {
            PrAction::Close => pr.state == PrState::Closed,
            PrAction::Reopen => pr.state != PrState::Closed,
            PrAction::MarkReady => !pr.draft,
            PrAction::Merge => self.merge_uses_auto_for(pr) && pr.auto_merge,
            PrAction::Approve => pr.viewer_approved,
        }
    }

    fn already_done_msg(&self, action: PrAction, (id, _): &(PrId, PR)) -> String {
        match action {
            PrAction::Close => format!("Already closed #{}", id.number),
            PrAction::Reopen => format!("Already open #{}", id.number),
            PrAction::MarkReady => format!("Already ready for review #{}", id.number),
            PrAction::Merge => format!("Auto-merge already enabled #{}", id.number),
            PrAction::Approve => format!("Already approved #{}", id.number),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Repo, Source};

    fn make_app() -> App {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        App::new(tx, crate::config::Config::default())
    }

    fn make_pr(number: u64) -> PR {
        PR {
            number,
            title: format!("pr {number}"),
            author: "alice".into(),
            draft: false,
            state: PrState::Open,
            created_at: "2024-01-01T00:00:00Z".into(),
            updated_at: "2024-01-01T00:00:00Z".into(),
            url: format!("https://github.com/owner/repo/pull/{number}"),
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

    /// Focus the per-repo PR list with a selected source/repo that has PRs enabled.
    fn setup_repo_prs(app: &mut App, prs: Vec<PR>) {
        app.sources = vec![Source::User("owner".into())];
        app.source_state.select(Some(0));
        app.source_ctx.repos = vec![Repo {
            name: "repo".into(),
            has_pull_requests: true,
            ..Repo::default()
        }];
        app.source_ctx.repo_state.select(Some(0));
        app.repos_view = ReposView::RepoList;
        app.repo_view = RepoView::Prs;
        app.focus = Column::Repo;
        app.repo_ctx.prs_raw = prs.clone();
        app.rebuild_prs();
    }

    /// Focus the source-level PR list with a selected source and some PRs.
    fn setup_source_prs(app: &mut App, prs: Vec<PR>) {
        app.sources = vec![Source::User("octocat".into())];
        app.source_state.select(Some(0));
        app.repos_view = ReposView::PrList;
        app.focus = Column::Repos;
        app.source_ctx.source_prs = prs.clone();
        app.source_ctx.source_pr_state.select(Some(0));
    }

    #[tokio::test]
    async fn hard_refresh_repo_prs_clears_selection() {
        let mut app = make_app();
        setup_repo_prs(&mut app, vec![make_pr(1), make_pr(2)]);
        let selected = app.pr_id_of(&app.repo_ctx.prs[0]);
        app.selected_prs.insert(selected);

        // The spawned fetch tasks race the test end; only the synchronous state
        // transition under test matters here.
        app.force_load_prs();

        assert!(
            app.selected_prs.is_empty(),
            "hard refresh must clear the PR selection"
        );
    }

    #[tokio::test]
    async fn hard_refresh_source_prs_clears_selection() {
        let mut app = make_app();
        let mut pr1 = make_pr(1);
        pr1.repo = "repo-a".into();
        let mut pr2 = make_pr(2);
        pr2.repo = "repo-b".into();
        setup_source_prs(&mut app, vec![pr1, pr2]);
        let selected = app.pr_id_of(&app.source_ctx.source_prs[0]);
        app.selected_prs.insert(selected);

        app.force_load_source_prs();

        assert!(
            app.selected_prs.is_empty(),
            "hard refresh must clear the source-level PR selection"
        );
    }

    #[tokio::test]
    async fn soft_refresh_source_prs_keeps_selection() {
        let mut app = make_app();
        let mut pr1 = make_pr(1);
        pr1.repo = "repo-a".into();
        setup_source_prs(&mut app, vec![pr1]);
        // Fresh cache: trigger_load_source_prs applies the cached list without fetching.
        app.source_prs_cache.insert(
            "octocat".into(),
            (std::time::Instant::now(), app.source_ctx.source_prs.clone()),
        );
        let selected = app.pr_id_of(&app.source_ctx.source_prs[0]);
        app.selected_prs.insert(selected);

        app.trigger_load_source_prs();

        assert!(
            app.pr_selection_active(),
            "a non-forced refresh must keep the selection"
        );
    }

    /// Regression: startup loads source 1's repos (cached), the user moves down to
    /// source 2 (its uncached fetch starts, setting `LoadKey::Repos`), then quickly back
    /// up to source 1. Source 1's cache hit must clear the stale key - otherwise the
    /// in-flight message for source 2 is discarded by its owner guard and nothing ever
    /// clears `LoadKey::Repos`, so "loading repos…" sticks forever.
    #[tokio::test]
    async fn cached_source_switch_clears_stale_repos_loading_key() {
        let mut app = make_app();
        // Startup state: two sources, source 1's repos already loaded and cached.
        app.sources = vec![Source::User("alice".into()), Source::User("bob".into())];
        app.source_state.select(Some(0));
        app.repo_cache.insert(
            ("alice".into(), app.repo_sort_key),
            (
                std::time::Instant::now(),
                vec![Repo {
                    name: "repo".into(),
                    has_pull_requests: true,
                    ..Repo::default()
                }],
            ),
        );

        // Move down to bob: not cached -> Repos key set + fetch spawned.
        app.source_state.select(Some(1));
        app.on_source_changed();
        assert!(app.loading_keys.contains(&LoadKey::Repos));

        // Move back up to alice: cache hit -> the stale key must go away.
        app.source_state.select(Some(0));
        app.on_source_changed();
        assert!(
            !app.loading_keys.contains(&LoadKey::Repos),
            "a cache hit must clear the stale Repos loading key"
        );

        // Bob's in-flight fetch finally lands and is discarded by its owner guard;
        // nothing may resurrect the spinner.
        app.handle_data(DataMsg::Repos {
            owner: "bob".into(),
            repos: vec![],
            has_more: false,
        });
        assert!(
            !app.loading_keys.contains(&LoadKey::Repos),
            "a discarded stale message must not leave the Repos key set"
        );
    }

    /// A cache hit with an empty repo list leaves no selection (the old else-branch);
    /// the key must still be cleared.
    #[tokio::test]
    async fn cached_empty_repos_clear_loading_key() {
        let mut app = make_app();
        app.sources = vec![Source::User("alice".into())];
        app.source_state.select(Some(0));
        app.repo_cache.insert(
            ("alice".into(), app.repo_sort_key),
            (std::time::Instant::now(), vec![]),
        );
        app.set_loading(LoadKey::Repos);

        app.trigger_load_repos();

        assert!(!app.loading_keys.contains(&LoadKey::Repos));
    }

    /// A filter that hides every source leaves nothing to fetch; an in-flight Repos
    /// key must not stick around.
    #[tokio::test]
    async fn trigger_load_repos_without_selected_source_clears_stale_key() {
        let mut app = make_app();
        app.sources = vec![Source::User("alice".into())];
        app.source_state.select(Some(0));
        app.set_loading(LoadKey::Repos);
        app.source_filter = "zzz".into();

        app.trigger_load_repos();

        assert!(!app.loading_keys.contains(&LoadKey::Repos));
    }

    /// Regression: source A's uncached PR fetch is in flight (SourcePrs key set). The user
    /// switches to the repo list, then moves to source B - in RepoList view no source-PR
    /// trigger runs, so only `invalidate_source` can drop A's stale key. Without that,
    /// A's in-flight message is discarded by its owner guard and the spinner sticks.
    #[tokio::test]
    async fn source_switch_clears_stale_source_prs_key() {
        let mut app = make_app();
        app.sources = vec![Source::User("alice".into()), Source::User("bob".into())];
        app.source_state.select(Some(0));
        app.repos_view = ReposView::PrList;
        app.focus = Column::Repos;

        // Alice's source-level PR fetch starts (no cache) -> key set.
        app.trigger_load_source_prs();
        assert!(app.loading_keys.contains(&LoadKey::SourcePrs));

        // Switch to the repo list, then move to bob. No source-PR trigger runs in this
        // view, so the key can only be cleared by the source invalidation.
        app.repos_view = ReposView::RepoList;
        app.source_state.select(Some(1));
        app.on_source_changed();
        assert!(
            !app.loading_keys.contains(&LoadKey::SourcePrs),
            "a source switch must clear the stale SourcePrs loading key"
        );

        // Alice's in-flight fetch finally lands and is discarded by its owner guard;
        // nothing may resurrect the spinner.
        app.handle_data(DataMsg::SourcePrs {
            owner: "alice".into(),
            prs: vec![],
            has_more: false,
        });
        assert!(
            !app.loading_keys.contains(&LoadKey::SourcePrs),
            "a discarded stale message must not leave the SourcePrs key set"
        );
    }

    /// Mirror of `source_switch_clears_stale_source_prs_key` for the source issue list.
    #[tokio::test]
    async fn source_switch_clears_stale_source_issues_key() {
        let mut app = make_app();
        app.sources = vec![Source::User("alice".into()), Source::User("bob".into())];
        app.source_state.select(Some(0));
        app.repos_view = ReposView::IssueList;
        app.focus = Column::Repos;

        // Alice's source-level issue fetch starts (no cache) -> key set.
        app.trigger_load_source_issues();
        assert!(app.loading_keys.contains(&LoadKey::SourceIssues));

        // Switch to the repo list, then move to bob. No source-issue trigger runs in this
        // view, so the key can only be cleared by the source invalidation.
        app.repos_view = ReposView::RepoList;
        app.source_state.select(Some(1));
        app.on_source_changed();
        assert!(
            !app.loading_keys.contains(&LoadKey::SourceIssues),
            "a source switch must clear the stale SourceIssues loading key"
        );

        // Alice's in-flight fetch finally lands and is discarded by its owner guard.
        app.handle_data(DataMsg::SourceIssues {
            owner: "alice".into(),
            issues: vec![],
            has_more: false,
        });
        assert!(
            !app.loading_keys.contains(&LoadKey::SourceIssues),
            "a discarded stale message must not leave the SourceIssues key set"
        );
    }

    /// Regression: repo A's PR fetch is in flight (RepoPrs key set). Moving to a source
    /// with no repos leaves `trigger_load_prs` without a selection; the key must be
    /// cleared by the repo/source invalidation, not left to a message that will never
    /// match.
    #[tokio::test]
    async fn source_switch_clears_stale_repo_prs_key() {
        let mut app = make_app();
        // alice has a repo with PRs enabled; bob has no repos at all.
        app.sources = vec![Source::User("alice".into()), Source::User("bob".into())];
        app.source_state.select(Some(0));
        app.repos_view = ReposView::RepoList;
        app.repo_view = RepoView::Prs;
        app.focus = Column::Repo;
        app.source_ctx.repos = vec![Repo {
            name: "repo".into(),
            has_pull_requests: true,
            ..Repo::default()
        }];
        app.source_ctx.repo_state.select(Some(0));

        // Repo A's PR fetch starts (no cache) -> key set.
        app.on_repo_changed();
        assert!(app.loading_keys.contains(&LoadKey::RepoPrs));

        // Move to bob: no repos, so no repo is selectable.
        app.source_state.select(Some(1));
        app.on_source_changed();
        assert!(
            !app.loading_keys.contains(&LoadKey::RepoPrs),
            "switching to a source with no repos must clear the stale RepoPrs key"
        );

        // Alice's in-flight fetch finally lands; the repo guard discards it.
        app.handle_data(DataMsg::Prs {
            repo: RepoId::new("alice", "repo"),
            prs: vec![],
            has_more: false,
        });
        assert!(
            !app.loading_keys.contains(&LoadKey::RepoPrs),
            "a discarded stale message must not leave the RepoPrs key set"
        );
    }

    /// Mirror of `source_switch_clears_stale_repo_prs_key` for the frontpage.
    #[tokio::test]
    async fn source_switch_clears_stale_frontpage_key() {
        let mut app = make_app();
        app.sources = vec![Source::User("alice".into()), Source::User("bob".into())];
        app.source_state.select(Some(0));
        app.repos_view = ReposView::RepoList;
        app.repo_view = RepoView::Frontpage;
        app.focus = Column::Repo;
        app.source_ctx.repos = vec![Repo {
            name: "repo".into(),
            has_pull_requests: true,
            ..Repo::default()
        }];
        app.source_ctx.repo_state.select(Some(0));

        // Frontpage fetch starts (no cache) -> key set.
        app.on_repo_changed();
        assert!(app.loading_keys.contains(&LoadKey::Frontpage));

        // Move to bob: no repos, so no repo is selectable.
        app.source_state.select(Some(1));
        app.on_source_changed();
        assert!(
            !app.loading_keys.contains(&LoadKey::Frontpage),
            "switching to a source with no repos must clear the stale Frontpage key"
        );

        // Alice's in-flight fetch finally lands; the repo guard discards it.
        app.handle_data(DataMsg::RepoFrontpage {
            repo: RepoId::new("alice", "repo"),
            description: String::new(),
            readme: String::new(),
        });
        assert!(
            !app.loading_keys.contains(&LoadKey::Frontpage),
            "a discarded stale message must not leave the Frontpage key set"
        );
    }

    /// Mirror of `source_switch_clears_stale_frontpage_key` for the issues view. A source
    /// change never re-triggers the issues load (only `on_repo_changed` does), so a
    /// source switch is the only thing that can clear an in-flight RepoIssues key.
    #[tokio::test]
    async fn source_switch_clears_stale_repo_issues_key() {
        let mut app = make_app();
        app.sources = vec![Source::User("alice".into()), Source::User("bob".into())];
        app.source_state.select(Some(0));
        app.repos_view = ReposView::RepoList;
        app.repo_view = RepoView::Issues;
        app.focus = Column::Repo;
        app.source_ctx.repos = vec![Repo {
            name: "repo".into(),
            has_issues: true,
            ..Repo::default()
        }];
        app.source_ctx.repo_state.select(Some(0));

        // Issues fetch starts (no cache) -> key set.
        app.on_repo_changed();
        assert!(app.loading_keys.contains(&LoadKey::RepoIssues));

        // Move to bob: no repos, so no repo is selectable.
        app.source_state.select(Some(1));
        app.on_source_changed();
        assert!(
            !app.loading_keys.contains(&LoadKey::RepoIssues),
            "switching to a source with no repos must clear the stale RepoIssues key"
        );

        // Alice's in-flight fetch finally lands; the repo guard discards it.
        app.handle_data(DataMsg::Issues {
            repo: RepoId::new("alice", "repo"),
            issues: vec![],
            has_more: false,
        });
        assert!(
            !app.loading_keys.contains(&LoadKey::RepoIssues),
            "a discarded stale message must not leave the RepoIssues key set"
        );
    }

    /// Regression: a repo's issues fetch is in flight (RepoIssues key set). The user
    /// focuses the Repos column and types a filter that hides every repo; no repo is
    /// selectable, `trigger_load_issues` returns early and must clear the key (the Issues
    /// message will be discarded by its repo guard).
    #[tokio::test]
    async fn filter_hiding_all_repos_clears_stale_issues_key() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app();
        app.sources = vec![Source::User("owner".into())];
        app.source_state.select(Some(0));
        app.repos_view = ReposView::RepoList;
        app.repo_view = RepoView::Issues;
        // Focus the Repos column so filter input targets the repo filter.
        app.focus = Column::Repos;
        app.source_ctx.repos = vec![Repo {
            name: "repo".into(),
            has_issues: true,
            ..Repo::default()
        }];
        app.source_ctx.repo_state.select(Some(0));

        // The issues fetch starts (no cache) -> key set.
        app.on_repo_changed();
        assert!(app.loading_keys.contains(&LoadKey::RepoIssues));

        // Type a filter that hides the only repo: no repo is selectable anymore.
        app.handle_filter_input(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));

        assert!(
            !app.loading_keys.contains(&LoadKey::RepoIssues),
            "hiding every repo with a filter must clear the stale RepoIssues key"
        );

        // The in-flight fetch finally lands; the repo guard discards it.
        app.handle_data(DataMsg::Issues {
            repo: RepoId::new("owner", "repo"),
            issues: vec![],
            has_more: false,
        });
        assert!(
            !app.loading_keys.contains(&LoadKey::RepoIssues),
            "a discarded stale message must not leave the RepoIssues key set"
        );
    }

    /// Regression: a diff for PR #1 is in flight (Action("diff") spinner up). Moving the
    /// cursor to PR #2 starts a new body load, which must drop the stale diff spinner -
    /// otherwise the in-flight DiffContent is discarded by its guard and "diff…" sticks.
    #[tokio::test]
    async fn switching_pr_clears_stale_diff_spinner() {
        let mut app = make_app();
        setup_repo_prs(&mut app, vec![make_pr(1), make_pr(2)]);

        // Open the diff for PR #1: spinner up.
        app.trigger_load_diff();
        assert!(app.loading_keys.contains(&LoadKey::Action("diff".into())));

        // Move to PR #2: a new body load starts and the stale diff spinner goes away.
        app.repo_ctx.pr_state.select(Some(1));
        app.trigger_load_pr_body();
        assert!(
            !app.loading_keys.contains(&LoadKey::Action("diff".into())),
            "loading a new PR body must clear the stale diff spinner"
        );

        // The in-flight diff for #1 finally lands; its guard discards it (a different PR is
        // selected) and nothing may resurrect the spinner.
        app.handle_data(DataMsg::DiffContent {
            pr: RepoId::new("owner", "repo").pr(1),
            title: String::new(),
            content: String::new(),
        });
        assert!(
            !app.loading_keys.contains(&LoadKey::Action("diff".into())),
            "a discarded stale diff must not leave its spinner set"
        );
    }

    /// Regression: a source-level fetch is in flight (SourcePrs key set for alice). The
    /// Sources message lands and re-selects a *different* source (the clamp moves the
    /// selection) — but that path never runs `on_source_changed()`, so no invalidation
    /// clear fires. The only thing that can drop the stale spinner is the owner guard
    /// discarding alice's in-flight message, which must clear it via identity match.
    #[tokio::test]
    async fn sources_reselect_clears_stale_source_prs_key() {
        let mut app = make_app();
        // Two sources so the clamp can move selection away from alice.
        app.sources = vec![Source::User("alice".into()), Source::User("bob".into())];
        app.source_state.select(Some(0));
        app.repos_view = ReposView::PrList;
        app.focus = Column::Repos;

        // Alice's source-level PR fetch starts (no cache) -> key set for alice.
        app.trigger_load_source_prs();
        assert!(app.loading_keys.contains(&LoadKey::SourcePrs));

        // The Sources message lands with only bob: the clamp moves selection to bob. This
        // path does NOT call on_source_changed(), so PR 1's invalidation clears never run.
        app.handle_data(DataMsg::Sources {
            sources: vec![Source::User("bob".into())],
            current_user: String::new(),
        });

        // Alice's in-flight fetch finally lands; the owner guard discards it and must
        // clear the stale SourcePrs key (identity: alice). Without this, the spinner sticks.
        app.handle_data(DataMsg::SourcePrs {
            owner: "alice".into(),
            prs: vec![],
            has_more: false,
        });
        assert!(
            !app.loading_keys.contains(&LoadKey::SourcePrs),
            "a discarded stale message after a Sources reselect must clear the SourcePrs key"
        );
    }

    /// Invariant: `clear_stale_loading` must never clear a key owned by a *different*
    /// identity. A stale message for repo A must not wipe the spinner of a newer, still-
    /// relevant fetch for repo B. This is what lets the discard guards clear unconditionally
    /// on stale messages without risking a false clear.
    #[tokio::test]
    async fn stale_message_does_not_clear_newer_repo_spinner() {
        let mut app = make_app();
        // alice has two repos; both have PRs enabled.
        app.sources = vec![Source::User("alice".into())];
        app.source_state.select(Some(0));
        app.repos_view = ReposView::RepoList;
        app.repo_view = RepoView::Prs;
        app.focus = Column::Repo;
        app.source_ctx.repos = vec![
            Repo {
                name: "repo-a".into(),
                has_pull_requests: true,
                ..Repo::default()
            },
            Repo {
                name: "repo-b".into(),
                has_pull_requests: true,
                ..Repo::default()
            },
        ];
        app.source_ctx.repo_state.select(Some(0));

        // Repo A's PR fetch starts (no cache) -> key set for repo-a.
        app.on_repo_changed();
        assert!(app.loading_keys.contains(&LoadKey::RepoPrs));

        // Switch to repo B: its fetch starts, so the RepoPrs key now belongs to repo-b.
        app.source_ctx.repo_state.select(Some(1));
        app.on_repo_changed();
        assert!(app.loading_keys.contains(&LoadKey::RepoPrs));

        // Repo A's stale in-flight fetch finally lands; the repo guard discards it. It must
        // NOT clear the RepoPrs key, which now belongs to repo-b's in-flight fetch.
        app.handle_data(DataMsg::Prs {
            repo: RepoId::new("alice", "repo-a"),
            prs: vec![],
            has_more: false,
        });
        assert!(
            app.loading_keys.contains(&LoadKey::RepoPrs),
            "a stale message for repo A must not clear the spinner of repo B's in-flight fetch"
        );

        // Repo B's own message lands and clears the key as usual.
        app.handle_data(DataMsg::Prs {
            repo: RepoId::new("alice", "repo-b"),
            prs: vec![],
            has_more: false,
        });
        assert!(
            !app.loading_keys.contains(&LoadKey::RepoPrs),
            "the current repo's own message must clear the RepoPrs key"
        );
    }
}
