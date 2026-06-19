mod commands;
mod fetch;

use super::App;
use crate::types::{Issue, RepoId, ReposView};

impl App {
    fn per_page(&self) -> u32 {
        let cfg = self.config.ui.per_page;
        if cfg == 0 {
            (u32::from(self.terminal_height) * 3 / 2).clamp(10, 50)
        } else {
            cfg.clamp(10, 100)
        }
    }

    pub(crate) fn selected_owner_repo(&self) -> Option<RepoId> {
        let owner = self.selected_source_owner()?;
        if self.repos_view == ReposView::PrList {
            let pr = self.selected_pr()?;
            let actual_owner = if pr.repo_owner.is_empty() {
                owner
            } else {
                pr.repo_owner.clone()
            };
            return Some(RepoId::new(actual_owner, pr.repo.clone()));
        }
        if self.repos_view == ReposView::IssueList {
            let issue = self.selected_source_issue()?;
            let actual_owner = if issue.repo_owner.is_empty() {
                owner
            } else {
                issue.repo_owner.clone()
            };
            return Some(RepoId::new(actual_owner, issue.repo.clone()));
        }
        let repo = self.selected_repo()?.to_string();
        Some(RepoId::new(owner, repo))
    }

    pub(crate) fn selected_source_issue(&self) -> Option<&Issue> {
        let visible = self.visible_source_issues();
        self.source_ctx
            .source_issue_state
            .selected()
            .and_then(|i| visible.get(i).copied())
    }
}
