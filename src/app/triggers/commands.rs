use super::super::App;
use crate::{
    actions,
    config::{CheckContext, IssueContext, Keybinding, PrContext, RepoContext},
    types::{DataMsg, RepoId, RepoView, ReposView},
};
use log::debug;

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
    debug!("{cmd} (clipboard)");
    let result = std::process::Command::new(cmd)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            if let Some(stdin) = c.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            c.wait()
        });
    if let Err(e) = result {
        debug!("{cmd} error: {e}");
    }
}

impl App {
    pub(super) fn spawn_keybinding_cmd(&mut self, name: String, cmd: String, output: KbOutput) {
        use crate::types::LoadingKind;
        self.loading = Some(LoadingKind::Action(name));
        let tx = self.tx.clone();
        tokio::spawn(async move {
            debug!("sh -c {cmd}");
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
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        debug!("sh -c {cmd} error: {}", stderr.trim());
                        DataMsg::Error(stderr.trim().to_string())
                    }
                    Err(e) => {
                        debug!("sh -c {cmd} error: {e}");
                        DataMsg::Error(e.to_string())
                    }
                },
                KbOutput::Silent => match out {
                    Ok(o) if o.status.success() => DataMsg::ActionDone(None),
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        debug!("sh -c {cmd} error: {}", stderr.trim());
                        DataMsg::ActionDone(Some(stderr.trim().to_string()))
                    }
                    Err(e) => {
                        debug!("sh -c {cmd} error: {e}");
                        DataMsg::ActionDone(Some(e.to_string()))
                    }
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

    fn selected_context_url(&self) -> Option<String> {
        use crate::types::Column;
        match self.focus {
            Column::Sources => {
                let owner = self.selected_source_owner()?;
                Some(format!("https://github.com/{owner}"))
            }
            Column::Repos => match self.repos_view {
                ReposView::PrList => self.selected_pr().map(|pr| pr.url.clone()),
                ReposView::IssueList => self.selected_issue().map(|i| i.url.clone()),
                ReposView::RepoList => self.selected_owner_repo().map(|rid| rid.url()),
            },
            Column::Repo | Column::Detail => match self.repo_view {
                RepoView::Frontpage => self.selected_owner_repo().map(|rid| rid.url()),
                RepoView::Issues => self.selected_issue().map(|i| i.url.clone()),
                RepoView::Prs => self.selected_pr().map(|pr| pr.url.clone()),
            },
        }
    }

    pub(crate) fn context_open_browser(&self) {
        if let Some(url) = self.selected_context_url() {
            self.spawn_open_url(&url);
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
        if let Some(url) = self.selected_context_url() {
            self.copy_and_notify(&url);
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
