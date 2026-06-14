mod commands;
mod fetch;

use super::App;
use crate::types::{RepoId, ReposView};

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
            return Some(RepoId::new(owner, pr.repo.clone()));
        }
        let repo = self.selected_repo()?.to_string();
        Some(RepoId::new(owner, repo))
    }
}
