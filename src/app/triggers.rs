use super::App;
use crate::{
    actions,
    config::{Keybinding, SourcesConfig},
    data::{
        fetch_check_runs, fetch_diff, fetch_issue_body, fetch_issues, fetch_pr_body, fetch_prs,
        fetch_rate_limit, fetch_repo_frontpage, fetch_repos, fetch_review_status, fetch_sources,
        rerun_check,
    },
    types::{Column, DataMsg, LoadingKind, PR, PrAction, RepoView},
};

enum KbOutput {
    Silent,
    CaptureStdout,
}

impl App {
    fn per_page(&self) -> u32 {
        let cfg = self.config.ui.per_page;
        if cfg == 0 {
            (self.terminal_height as u32 * 3 / 2).clamp(10, 50)
        } else {
            cfg.clamp(10, 100)
        }
    }

    pub(crate) fn selected_owner_repo(&self) -> Option<(String, String)> {
        let owner = self.selected_source_owner()?;
        let repo = self.selected_repo()?.to_string();
        Some((owner, repo))
    }

    fn spawn_keybinding_cmd(&mut self, name: String, cmd: String, output: KbOutput) {
        self.loading = Some(LoadingKind::Action(name));
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let out = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .output()
                .await;
            let msg = match output {
                KbOutput::CaptureStdout => match out {
                    Ok(o) if o.status.success() => DataMsg::ActionDone(Some(
                        String::from_utf8_lossy(&o.stdout).trim().to_string(),
                    )),
                    Ok(o) => DataMsg::Error(String::from_utf8_lossy(&o.stderr).trim().to_string()),
                    Err(e) => DataMsg::Error(e.to_string()),
                },
                KbOutput::Silent => match out {
                    Ok(o) if o.status.success() => DataMsg::ActionDone(None),
                    Ok(o) => DataMsg::ActionDone(Some(
                        String::from_utf8_lossy(&o.stderr).trim().to_string(),
                    )),
                    Err(e) => DataMsg::ActionDone(Some(e.to_string())),
                },
            };
            let _ = tx.send(msg);
        });
    }

    fn dispatch_keybinding_cmd(
        &mut self,
        kb: &Keybinding,
        cmd: String,
        output: KbOutput,
    ) -> Option<String> {
        if kb.interactive {
            return Some(cmd);
        }
        let name = kb.name.as_deref().unwrap_or("custom").to_string();
        self.spawn_keybinding_cmd(name, cmd, output);
        None
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

        if let Some((fetched_at, cached)) = self.repo_cache.get(&owner).cloned() {
            if fetched_at.elapsed() < self.config.cache_ttl() {
                self.repos_page = 1;
                self.repos_has_more = cached.len() == per_page as usize;
                self.repos_fetching_more = false;
                self.apply_repos(cached);
                if self.repo_state.selected().is_some() {
                    self.trigger_load_prs();
                }
                return;
            }
            self.apply_repos(cached);
        }

        let current_user = self.current_user.clone().unwrap_or_default();
        self.loading = Some(LoadingKind::Repos);
        self.repos_fetching_more = false;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_repos(&source, &current_user, per_page, 1).await {
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
            self.repo_cache.remove(&owner);
        }
        self.trigger_load_repos();
    }

    pub(crate) fn trigger_load_more_repos(&mut self) {
        if self.repos_fetching_more || !self.repos_has_more {
            return;
        }
        let Some(source) = self.selected_source().cloned() else {
            return;
        };
        let owner = source.owner().to_string();
        let current_user = self.current_user.clone().unwrap_or_default();
        let per_page = self.per_page();
        self.loading = Some(LoadingKind::Repos);
        let page = self.repos_page + 1;
        self.repos_page = page;
        self.repos_fetching_more = true;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_repos(&source, &current_user, per_page, page).await {
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

    pub(crate) fn trigger_load_pr_body(&mut self) {
        let Some((owner, repo)) = self.selected_owner_repo() else {
            return;
        };
        let Some(pr) = self.selected_pr() else { return };
        let pr_number = pr.number;
        let sha = pr.head_sha.clone();
        self.clear_pr_detail();
        let tx = self.tx.clone();
        let o = owner.clone();
        let r = repo.clone();
        tokio::spawn(async move {
            let (body, mergeable_state, additions, deletions) =
                fetch_pr_body(&o, &r, pr_number).await.unwrap_or_default();
            let _ = tx.send(DataMsg::PrBody {
                pr_number,
                body,
                mergeable_state,
                additions,
                deletions,
            });
        });
        let tx2 = self.tx.clone();
        tokio::spawn(async move {
            let runs = fetch_check_runs(&owner, &repo, &sha).await;
            let _ = tx2.send(DataMsg::CheckRuns { pr_number, runs });
        });
    }

    pub(crate) fn trigger_load_prs(&mut self) {
        let Some((owner, repo)) = self.selected_owner_repo() else {
            return;
        };
        self.clear_pr_detail();
        self.mergeable_states.clear();
        self.repo_frontpage = None;
        self.repo_frontpage_scroll = 0;
        let key = format!("{owner}/{repo}");

        if let Some((fetched_at, cached)) = self.pr_cache.get(&key).cloned() {
            if fetched_at.elapsed() < self.config.cache_ttl() {
                self.apply_prs(cached);
                return;
            }
            // Stale cache: show existing data, refresh silently in background.
            self.apply_prs(cached);
            let per_page = self.per_page();
            let tx = self.tx.clone();
            tokio::spawn(async move {
                match fetch_prs(&owner, &repo, per_page, 1).await {
                    Ok(prs) => {
                        let has_more = prs.len() == per_page as usize;
                        let _ = tx.send(DataMsg::Prs {
                            owner,
                            repo,
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

        self.loading = Some(LoadingKind::Prs);
        self.clear_pr_state();
        self.prs_fetching_more = false;
        let per_page = self.per_page();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_prs(&owner, &repo, per_page, 1).await {
                Ok(prs) => {
                    let has_more = prs.len() == per_page as usize;
                    let _ = tx.send(DataMsg::Prs {
                        owner,
                        repo,
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
        if self.prs_fetching_more || !self.prs_has_more {
            return;
        }
        let Some((owner, repo)) = self.selected_owner_repo() else {
            return;
        };
        let per_page = self.per_page();
        let page = self.prs_page + 1;
        self.prs_page = page;
        self.loading = Some(LoadingKind::Prs);
        self.prs_fetching_more = true;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_prs(&owner, &repo, per_page, page).await {
                Ok(prs) => {
                    let has_more = prs.len() == per_page as usize;
                    let _ = tx.send(DataMsg::MorePrs {
                        owner,
                        repo,
                        prs,
                        page,
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
        let Some((owner, repo)) = self.selected_owner_repo() else {
            return;
        };
        let key = format!("{owner}/{repo}");
        let tx = self.tx.clone();

        let existing_reviews = self.review_cache.get(&key).cloned().unwrap_or_default();
        let prs_to_fetch: Vec<PR> = self
            .prs
            .iter()
            .filter(|pr| !existing_reviews.contains_key(&pr.number))
            .cloned()
            .collect();

        if prs_to_fetch.is_empty() {
            return;
        }

        let owner: std::sync::Arc<str> = owner.into();
        let repo: std::sync::Arc<str> = repo.into();

        for pr in prs_to_fetch {
            let o = owner.clone();
            let r = repo.clone();
            let tx2 = tx.clone();
            let num = pr.number;
            tokio::spawn(async move {
                let status = fetch_review_status(&o, &r, num).await;
                let _ = tx2.send(DataMsg::ReviewStatus {
                    owner: o.to_string(),
                    repo: r.to_string(),
                    pr_number: num,
                    status,
                });
            });
        }
    }

    pub(crate) fn trigger_load_frontpage(&mut self) {
        let Some((owner, repo)) = self.selected_owner_repo() else {
            return;
        };
        self.repo_frontpage = None;
        self.repo_frontpage_scroll = 0;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if let Ok((description, readme)) = fetch_repo_frontpage(&owner, &repo).await {
                let _ = tx.send(DataMsg::RepoFrontpage {
                    owner,
                    repo,
                    description,
                    readme,
                });
            }
        });
    }

    pub(crate) fn trigger_load_issues(&mut self) {
        let Some((owner, repo)) = self.selected_owner_repo() else {
            return;
        };
        self.clear_issue_state();
        self.loading = Some(LoadingKind::Issues);
        self.issues_fetching_more = false;
        let per_page = self.per_page();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_issues(&owner, &repo, per_page, 1).await {
                Ok((issues, has_more)) => {
                    let _ = tx.send(DataMsg::Issues {
                        owner,
                        repo,
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
        if self.issues_fetching_more || !self.issues_has_more {
            return;
        }
        let Some((owner, repo)) = self.selected_owner_repo() else {
            return;
        };
        let per_page = self.per_page();
        let page = self.issues_page + 1;
        self.issues_page = page;
        self.loading = Some(LoadingKind::Issues);
        self.issues_fetching_more = true;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_issues(&owner, &repo, per_page, page).await {
                Ok((issues, has_more)) => {
                    let _ = tx.send(DataMsg::MoreIssues {
                        owner,
                        repo,
                        issues,
                        page,
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
        let Some((owner, repo)) = self.selected_owner_repo() else {
            return;
        };
        let Some(number) = self.selected_issue().map(|i| i.number) else {
            return;
        };
        self.issue_body = None;
        self.issue_body_scroll = 0;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if let Ok(body) = fetch_issue_body(&owner, &repo, number).await {
                let _ = tx.send(DataMsg::IssueBody { number, body });
            }
        });
    }

    pub(crate) fn dispatch_repo_view_trigger(&mut self) {
        match self.repo_view {
            RepoView::Frontpage => self.trigger_load_frontpage(),
            RepoView::Prs => {
                if self.prs.is_empty() {
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
        self.dispatch_repo_view_trigger();
    }

    pub(crate) fn trigger_refresh(&mut self) {
        match self.focus {
            Column::Sources => self.trigger_load_sources(),
            Column::Repos => self.force_load_repos(),
            Column::Repo | Column::Detail => match self.repo_view {
                RepoView::Frontpage => self.trigger_load_frontpage(),
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
        let Some((owner, repo)) = self.selected_owner_repo() else {
            return;
        };
        let Some(pr) = self.selected_pr().cloned() else {
            return;
        };
        let title = format!("#{} {}", pr.number, pr.title);
        self.loading = Some(LoadingKind::Action("diff".into()));
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_diff(&owner, &repo, pr.number).await {
                Ok(content) => {
                    let _ = tx.send(DataMsg::DiffContent { title, content });
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn do_pr_action(&mut self, action: PrAction) {
        let Some((owner, repo)) = self.selected_owner_repo() else {
            return;
        };
        let Some(pr) = self.selected_pr().cloned() else {
            return;
        };

        let tx = self.tx.clone();
        self.loading = Some(LoadingKind::Action(action.label().into()));

        let merge_method = self.config.ui.merge_method;
        tokio::spawn(async move {
            let result = match action {
                PrAction::Approve => actions::approve(&owner, &repo, pr.number).await,
                PrAction::Merge => actions::merge(&owner, &repo, pr.number, merge_method).await,
                PrAction::Close => actions::close_pr(&owner, &repo, pr.number).await,
                PrAction::Reopen => actions::reopen_pr(&owner, &repo, pr.number).await,
                PrAction::MarkReady => actions::mark_ready(&owner, &repo, pr.number).await,
            }
            .map(|()| Some(action.success_msg(pr.number)));
            match result {
                Ok(msg) => {
                    let _ = tx.send(DataMsg::ActionDone(msg));
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn context_open_browser(&self) {
        match self.focus {
            Column::Sources => {
                let Some(owner) = self.selected_source_owner() else {
                    return;
                };
                self.spawn_open_url(&format!("https://github.com/{owner}"));
            }
            Column::Repos => {
                let Some((owner, repo)) = self.selected_owner_repo() else {
                    return;
                };
                self.spawn_open_url(&format!("https://github.com/{owner}/{repo}"));
            }
            Column::Repo | Column::Detail => match self.repo_view {
                RepoView::Frontpage => {
                    let Some((owner, repo)) = self.selected_owner_repo() else {
                        return;
                    };
                    self.spawn_open_url(&format!("https://github.com/{owner}/{repo}"));
                }
                RepoView::Issues => {
                    if let Some(issue) = self.selected_issue() {
                        self.spawn_open_url(&issue.url);
                    }
                }
                RepoView::Prs => {
                    if let Some(pr) = self.selected_pr() {
                        self.spawn_open_url(&pr.url);
                    }
                }
            },
        }
    }

    pub(crate) fn context_open_issues(&self) {
        if self.focus == Column::Repos {
            let Some((owner, repo)) = self.selected_owner_repo() else {
                return;
            };
            self.spawn_open_url(&format!("https://github.com/{owner}/{repo}/issues"));
        }
    }

    pub(crate) fn context_copy_url(&mut self) {
        match self.focus {
            Column::Sources => {
                let Some(owner) = self.selected_source_owner() else {
                    return;
                };
                let url = format!("https://github.com/{owner}");
                copy_to_clipboard(&url);
            }
            Column::Repos => {
                let Some((owner, repo)) = self.selected_owner_repo() else {
                    return;
                };
                let url = format!("https://github.com/{owner}/{repo}");
                copy_to_clipboard(&url);
            }
            Column::Repo | Column::Detail => match self.repo_view {
                RepoView::Frontpage => {
                    let Some((owner, repo)) = self.selected_owner_repo() else {
                        return;
                    };
                    self.copy_and_notify(format!("https://github.com/{owner}/{repo}"));
                }
                RepoView::Prs => {
                    let Some((owner, repo)) = self.selected_owner_repo() else {
                        return;
                    };
                    if let Some(number) = self.selected_pr().map(|p| p.number) {
                        self.copy_and_notify(format!(
                            "https://github.com/{owner}/{repo}/pull/{number}"
                        ));
                    }
                }
                RepoView::Issues => {
                    let Some((owner, repo)) = self.selected_owner_repo() else {
                        return;
                    };
                    if let Some(number) = self.selected_issue().map(|i| i.number) {
                        self.copy_and_notify(format!(
                            "https://github.com/{owner}/{repo}/issues/{number}"
                        ));
                    }
                }
            },
        }
    }

    fn copy_and_notify(&mut self, text: String) {
        self.set_status(format!("Copied: {text}"));
        copy_to_clipboard(&text);
    }

    fn spawn_open_url(&self, url: &str) {
        if let Err(e) = actions::open_url(url) {
            let _ = self.tx.send(DataMsg::Error(e.to_string()));
        }
    }

    pub(crate) fn post_dependabot_comment(&mut self, body: &str) {
        let Some((owner, repo)) = self.selected_owner_repo() else {
            return;
        };
        let Some(pr) = self.selected_pr().cloned() else {
            return;
        };
        let body = body.to_string();
        let tx = self.tx.clone();
        self.loading = Some(LoadingKind::Action("comment".into()));
        tokio::spawn(async move {
            let result = actions::post_comment(&owner, &repo, pr.number, &body).await;
            match result {
                Ok(()) => {
                    let _ = tx.send(DataMsg::ActionDone(Some(format!("Sent: {body}"))));
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub(crate) fn diff_scroll(&mut self, n: u16) {
        if let Some(d) = &mut self.diff_view {
            let max = u16::try_from(d.lines.len().saturating_sub(1)).unwrap_or(u16::MAX);
            d.scroll = (d.scroll + n).min(max);
        }
    }

    pub(crate) fn diff_scroll_up(&mut self, n: u16) {
        if let Some(d) = &mut self.diff_view {
            d.scroll = d.scroll.saturating_sub(n);
        }
    }

    pub fn trigger_keybinding_pr(&mut self, kb: &Keybinding) -> Option<String> {
        let owner = self.selected_source_owner()?;
        let repo = self.selected_repo()?.to_string();
        let pr = self.selected_pr()?.clone();
        let cmd = kb.expand_command_pr(&pr, &owner, &repo)?;
        self.dispatch_keybinding_cmd(kb, cmd, KbOutput::Silent)
    }

    pub(crate) fn open_selected_check(&mut self) {
        let Some(idx) = self.check_runs_state.selected() else {
            return;
        };
        let Some(runs) = &self.check_runs else { return };
        let Some(run) = runs.get(idx) else { return };
        self.spawn_open_url(&run.url);
    }

    pub(crate) fn rerun_selected_check(&mut self) {
        let Some(idx) = self.check_runs_state.selected() else {
            return;
        };
        let Some(runs) = &self.check_runs else { return };
        let Some(run) = runs.get(idx) else { return };
        let check_run_id = run.id;
        let name = run.name.clone();
        let Some((owner, repo)) = self.selected_owner_repo() else {
            return;
        };
        self.set_status(format!("Re-running {name}…"));
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match rerun_check(&owner, &repo, check_run_id).await {
                Ok(()) => {
                    let _ = tx.send(DataMsg::ActionDone(Some(format!("Re-running {name}"))));
                }
                Err(e) => {
                    let _ = tx.send(DataMsg::Error(e.to_string()));
                }
            }
        });
    }

    pub fn trigger_keybinding_check(&mut self, kb: &Keybinding) -> Option<String> {
        let idx = self.check_runs_state.selected()?;
        let runs = self.check_runs.as_ref()?;
        let run = runs.get(idx)?;
        let pr = self.selected_pr()?;
        let pr_number = pr.number;
        let owner = self.selected_source_owner()?;
        let repo = self.selected_repo()?.to_string();
        let cmd = kb.expand_command_check(run, pr_number, &owner, &repo)?;
        self.dispatch_keybinding_cmd(kb, cmd, KbOutput::CaptureStdout)
    }

    pub fn trigger_keybinding_repo(&mut self, kb: &Keybinding) -> Option<String> {
        let owner = self.selected_source_owner()?;
        let repo = self.selected_repo()?.to_string();
        let lang = self
            .repos
            .iter()
            .find(|r| r.name == repo)
            .and_then(|r| r.language.as_deref());
        let cmd = kb.expand_command_repo(&owner, &repo, lang)?;
        self.dispatch_keybinding_cmd(kb, cmd, KbOutput::Silent)
    }
}

fn copy_to_clipboard(text: &str) {
    let cmd = if cfg!(target_os = "macos") {
        "pbcopy"
    } else {
        "xclip"
    };
    let _ = std::process::Command::new(cmd)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            if let Some(stdin) = c.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            c.wait()
        });
}
