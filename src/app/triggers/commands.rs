use super::super::App;
use crate::{
    actions,
    config::{CheckContext, IssueContext, Keybinding, PrContext, RepoContext},
    types::{DataMsg, RepoId, RepoView, ReposView},
};

pub(super) enum KbOutput {
    Silent,
    CaptureStdout,
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

impl App {
    pub(super) fn spawn_keybinding_cmd(&mut self, name: String, cmd: String, output: KbOutput) {
        use crate::types::LoadingKind;
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

    pub(super) fn dispatch_keybinding_cmd(
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

    pub(crate) fn context_open_browser(&self) {
        use crate::types::{Column, ReposView};
        match self.focus {
            Column::Sources => {
                let Some(owner) = self.selected_source_owner() else {
                    return;
                };
                self.spawn_open_url(&format!("https://github.com/{owner}"));
            }
            Column::Repos => match self.repos_view {
                ReposView::PrList => {
                    if let Some(pr) = self.selected_pr() {
                        self.spawn_open_url(&pr.url);
                    }
                }
                ReposView::IssueList => {
                    if let Some(issue) = self.selected_issue() {
                        self.spawn_open_url(&issue.url);
                    }
                }
                ReposView::RepoList => {
                    let Some(rid) = self.selected_owner_repo() else {
                        return;
                    };
                    self.spawn_open_url(&rid.url());
                }
            },
            Column::Repo | Column::Detail => match self.repo_view {
                RepoView::Frontpage => {
                    let Some(rid) = self.selected_owner_repo() else {
                        return;
                    };
                    self.spawn_open_url(&rid.url());
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
        use crate::types::Column;
        if self.focus == Column::Repos {
            let Some(rid) = self.selected_owner_repo() else {
                return;
            };
            self.spawn_open_url(&rid.issues_url());
        }
    }

    pub(crate) fn context_copy_url(&mut self) {
        use crate::types::Column;
        match self.focus {
            Column::Sources => {
                let Some(owner) = self.selected_source_owner() else {
                    return;
                };
                let url = format!("https://github.com/{owner}");
                copy_to_clipboard(&url);
            }
            Column::Repos => match self.repos_view {
                ReposView::PrList => {
                    if let Some(url) = self.selected_pr().map(|pr| pr.url.clone()) {
                        self.copy_and_notify(&url);
                    }
                }
                ReposView::IssueList => {
                    if let Some(url) = self.selected_issue().map(|i| i.url.clone()) {
                        self.copy_and_notify(&url);
                    }
                }
                ReposView::RepoList => {
                    let Some(rid) = self.selected_owner_repo() else {
                        return;
                    };
                    self.copy_and_notify(&rid.url());
                }
            },
            Column::Repo | Column::Detail => match self.repo_view {
                RepoView::Frontpage => {
                    let Some(rid) = self.selected_owner_repo() else {
                        return;
                    };
                    self.copy_and_notify(&rid.url());
                }
                RepoView::Prs => {
                    if let Some((_, pr)) = self.selected_pr_context() {
                        self.copy_and_notify(&pr.url);
                    }
                }
                RepoView::Issues => {
                    if let Some((_, issue)) = self.selected_issue_context() {
                        self.copy_and_notify(&issue.url);
                    }
                }
            },
        }
    }

    fn copy_and_notify(&mut self, text: &str) {
        self.set_status(format!("Copied: {text}"));
        copy_to_clipboard(text);
    }

    pub(crate) fn spawn_open_url(&self, url: &str) {
        if let Err(e) = actions::open_url(url) {
            let _ = self.tx.send(DataMsg::Error(e.to_string()));
        }
    }

    pub(crate) fn post_dependabot_comment(&mut self, body: &str) {
        let Some(pr_id) = self.selected_pr_id() else {
            return;
        };
        let body = body.to_string();
        let tx = self.tx.clone();
        use crate::types::LoadingKind;
        self.loading = Some(LoadingKind::Action("comment".into()));
        tokio::spawn(async move {
            let result = actions::post_comment(&pr_id, &body).await;
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

    pub(crate) fn open_selected_check(&mut self) {
        let Some(idx) = self.repo_ctx.check_runs_state.selected() else {
            return;
        };
        let Some(runs) = &self.repo_ctx.check_runs else {
            return;
        };
        let Some(run) = runs.get(idx) else { return };
        self.spawn_open_url(&run.url);
    }

    pub fn trigger_keybinding_pr(&mut self, kb: &Keybinding) -> Option<String> {
        let (RepoId { owner, repo }, pr) = self.selected_pr_context()?;
        let cmd = kb.expand_command(&PrContext {
            pr: &pr,
            owner: &owner,
            repo: &repo,
        })?;
        self.dispatch_keybinding_cmd(kb, cmd, KbOutput::Silent)
    }

    pub fn trigger_keybinding_check(&mut self, kb: &Keybinding) -> Option<String> {
        let idx = self.repo_ctx.check_runs_state.selected()?;
        let runs = self.repo_ctx.check_runs.as_ref()?;
        let run = runs.get(idx)?;
        let pr = self.selected_pr()?;
        let pr_number = pr.number;
        let owner = self.selected_source_owner()?;
        let repo = self.selected_repo()?.to_string();
        let cmd = kb.expand_command(&CheckContext {
            run,
            pr_number,
            owner: &owner,
            repo: &repo,
        })?;
        self.dispatch_keybinding_cmd(kb, cmd, KbOutput::CaptureStdout)
    }

    pub fn trigger_keybinding_issue(&mut self, kb: &Keybinding) -> Option<String> {
        let issue = self.selected_issue()?.clone();
        let owner = if issue.repo_owner.is_empty() {
            self.selected_source_owner()?
        } else {
            issue.repo_owner.clone()
        };
        let repo = if issue.repo.is_empty() {
            self.selected_repo()?.to_string()
        } else {
            issue.repo.clone()
        };
        let cmd = kb.expand_command(&IssueContext {
            issue: &issue,
            owner: &owner,
            repo: &repo,
        })?;
        self.dispatch_keybinding_cmd(kb, cmd, KbOutput::Silent)
    }

    pub fn trigger_keybinding_repo(&mut self, kb: &Keybinding) -> Option<String> {
        let owner = self.selected_source_owner()?;
        let repo = self.selected_repo()?.to_string();
        let lang = self
            .source_ctx
            .repos
            .iter()
            .find(|r| r.name == repo)
            .and_then(|r| r.language.as_deref());
        let cmd = kb.expand_command(&RepoContext {
            owner: &owner,
            repo: &repo,
            language: lang,
        })?;
        self.dispatch_keybinding_cmd(kb, cmd, KbOutput::Silent)
    }
}
