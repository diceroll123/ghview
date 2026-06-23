use super::App;
use crate::types::{Column, DetailSection, RepoView, ReposView};
use ratatui::widgets::ListState;

fn clamp_list_state(state: &mut ListState, len: usize) {
    if let Some(i) = state.selected()
        && i >= len
    {
        state.select(if len > 0 { Some(len - 1) } else { None });
    }
}

trait SelectChanged {
    fn select_changed(&mut self, i: Option<usize>) -> bool;
    fn nav_prev(&mut self, len: usize) -> bool;
    fn nav_next(&mut self, len: usize) -> bool;
}

impl SelectChanged for ListState {
    fn select_changed(&mut self, i: Option<usize>) -> bool {
        let prev = self.selected();
        self.select(i);
        self.selected() != prev
    }

    fn nav_prev(&mut self, len: usize) -> bool {
        if len == 0 {
            return false;
        }
        let idx = self.selected().map_or(0, |i| i.saturating_sub(1));
        self.select_changed(Some(idx))
    }

    fn nav_next(&mut self, len: usize) -> bool {
        if len == 0 {
            return false;
        }
        let idx = self.selected().map_or(0, |i| (i + 1).min(len - 1));
        self.select_changed(Some(idx))
    }
}

impl App {
    pub(crate) fn clamp_source_selection(&mut self) {
        let len = self.visible_sources().len();
        clamp_list_state(&mut self.source_state, len);
    }

    pub(crate) fn clamp_repo_selection(&mut self) {
        let len = self.visible_repos().len();
        clamp_list_state(&mut self.source_ctx.repo_state, len);
    }

    pub(crate) fn clamp_source_pr_selection(&mut self) {
        let len = self.visible_source_prs().len();
        clamp_list_state(&mut self.source_ctx.source_pr_state, len);
    }

    pub(crate) fn clamp_source_issue_selection(&mut self) {
        let len = self.visible_source_issues().len();
        clamp_list_state(&mut self.source_ctx.source_issue_state, len);
    }

    pub(crate) fn on_source_changed(&mut self) {
        self.invalidate_source();
        self.trigger_load_repos();
        match self.repos_view {
            ReposView::PrList => self.trigger_load_source_prs(),
            ReposView::IssueList => self.trigger_load_source_issues(),
            ReposView::RepoList => {}
        }
    }

    pub(crate) fn on_repo_changed(&mut self) {
        self.pr_filter.clear();
        self.invalidate_repo();

        self.trigger_load_prs(); // keep PRs loaded for PR tab count

        match self.repo_view {
            RepoView::Frontpage => self.trigger_load_frontpage(),
            RepoView::Issues => self.trigger_load_issues(),
            RepoView::Prs => {}
        }
    }

    pub(crate) fn move_up(&mut self) {
        match self.focus {
            Column::Sources => {
                let len = self.visible_sources().len();
                if self.source_state.nav_prev(len) {
                    self.on_source_changed();
                }
            }
            Column::Repos => match self.repos_view {
                ReposView::RepoList => {
                    let len = self.visible_repos().len();
                    if self.source_ctx.repo_state.nav_prev(len) {
                        self.on_repo_changed();
                    }
                }
                ReposView::PrList => {
                    let len = self.visible_source_prs().len();
                    if self.source_ctx.source_pr_state.nav_prev(len) {
                        self.trigger_load_pr_body();
                    }
                }
                ReposView::IssueList => {
                    let len = self.visible_source_issues().len();
                    if self.source_ctx.source_issue_state.nav_prev(len) {
                        self.trigger_load_source_issue_body();
                    }
                }
            },
            Column::Repo => match self.repo_view {
                RepoView::Frontpage => {
                    self.repo_ctx.repo_frontpage_scroll =
                        self.repo_ctx.repo_frontpage_scroll.saturating_sub(1);
                }
                RepoView::Prs => {
                    if self.repo_ctx.pr_state.nav_prev(self.repo_ctx.prs.len()) {
                        self.trigger_load_pr_body();
                    }
                }
                RepoView::Issues => {
                    if self
                        .repo_ctx
                        .issue_state
                        .nav_prev(self.repo_ctx.issues.len())
                    {
                        self.trigger_load_issue_body();
                    }
                }
            },
            Column::Detail => match self.repo_view {
                RepoView::Issues => {
                    self.repo_ctx.issue_body_scroll =
                        self.repo_ctx.issue_body_scroll.saturating_sub(1);
                }
                RepoView::Prs | RepoView::Frontpage => match self.repo_ctx.detail_section {
                    DetailSection::Body => {
                        self.repo_ctx.pr_body_scroll =
                            self.repo_ctx.pr_body_scroll.saturating_sub(1);
                    }
                    DetailSection::Checks => {
                        let len = self.repo_ctx.check_runs.as_ref().map_or(0, Vec::len);
                        self.repo_ctx.check_runs_state.nav_prev(len);
                    }
                },
            },
        }
    }

    pub(crate) fn move_down(&mut self) {
        match self.focus {
            Column::Sources => {
                let len = self.visible_sources().len();
                if self.source_state.nav_next(len) {
                    self.on_source_changed();
                }
            }
            Column::Repos => match self.repos_view {
                ReposView::RepoList => {
                    let len = self.visible_repos().len();
                    let at_last = len > 0 && self.source_ctx.repo_state.selected() == Some(len - 1);
                    if self.source_ctx.repo_state.nav_next(len) {
                        self.on_repo_changed();
                    }
                    if at_last && self.source_ctx.repo_filter.is_empty() {
                        self.trigger_load_more_repos();
                    }
                }
                ReposView::PrList => {
                    let len = self.visible_source_prs().len();
                    let at_last = len > 0
                        && self.source_ctx.source_pr_state.selected() == Some(len - 1)
                        && self.source_ctx.source_pr_filter.is_empty();
                    if self.source_ctx.source_pr_state.nav_next(len) {
                        self.trigger_load_pr_body();
                    }
                    if at_last {
                        self.trigger_load_more_source_prs();
                    }
                }
                ReposView::IssueList => {
                    let len = self.visible_source_issues().len();
                    let at_last = len > 0
                        && self.source_ctx.source_issue_state.selected() == Some(len - 1)
                        && self.source_ctx.source_issue_filter.is_empty();
                    if self.source_ctx.source_issue_state.nav_next(len) {
                        self.trigger_load_source_issue_body();
                    }
                    if at_last {
                        self.trigger_load_more_source_issues();
                    }
                }
            },
            Column::Repo => match self.repo_view {
                RepoView::Frontpage => {
                    self.repo_ctx.repo_frontpage_scroll =
                        self.repo_ctx.repo_frontpage_scroll.saturating_add(1);
                }
                RepoView::Prs => {
                    let len = self.repo_ctx.prs.len();
                    let at_last = len > 0 && self.repo_ctx.pr_state.selected() == Some(len - 1);
                    if self.repo_ctx.pr_state.nav_next(len) {
                        self.trigger_load_pr_body();
                    }
                    if at_last && self.pr_filter.is_empty() {
                        self.trigger_load_more_prs();
                    }
                }
                RepoView::Issues => {
                    let len = self.repo_ctx.issues.len();
                    let at_last = len > 0 && self.repo_ctx.issue_state.selected() == Some(len - 1);
                    if self.repo_ctx.issue_state.nav_next(len) {
                        self.trigger_load_issue_body();
                    }
                    if at_last {
                        self.trigger_load_more_issues();
                    }
                }
            },
            Column::Detail => match self.repo_view {
                RepoView::Issues => {
                    self.repo_ctx.issue_body_scroll =
                        self.repo_ctx.issue_body_scroll.saturating_add(1);
                }
                RepoView::Prs | RepoView::Frontpage => match self.repo_ctx.detail_section {
                    DetailSection::Body => {
                        self.repo_ctx.pr_body_scroll =
                            self.repo_ctx.pr_body_scroll.saturating_add(1);
                    }
                    DetailSection::Checks => {
                        let len = self.repo_ctx.check_runs.as_ref().map_or(0, Vec::len);
                        self.repo_ctx.check_runs_state.nav_next(len);
                    }
                },
            },
        }
    }

    pub(crate) fn move_left(&mut self) {
        match self.focus {
            Column::Repos => self.focus = Column::Sources,
            Column::Repo => self.focus = Column::Repos,
            Column::Detail => {
                if matches!(self.repos_view, ReposView::PrList | ReposView::IssueList) {
                    self.focus = Column::Repos;
                } else {
                    self.focus = Column::Repo;
                }
            }
            Column::Sources => {}
        }
    }

    pub(crate) fn move_right(&mut self) {
        match self.focus {
            Column::Sources => {
                if self.selected_source().is_some() {
                    self.focus = Column::Repos;
                    if self.source_ctx.repos.is_empty() {
                        self.trigger_load_repos();
                    }
                }
            }
            Column::Repos => match self.repos_view {
                ReposView::RepoList => {
                    if self.selected_repo().is_some() {
                        self.focus = Column::Repo;
                        if (self.repo_view == RepoView::Prs && !self.selected_repo_has_prs())
                            || (self.repo_view == RepoView::Issues
                                && !self.selected_repo_has_issues())
                        {
                            self.repo_view = RepoView::Frontpage;
                        }
                        self.repo_ctx.pr_body_scroll = 0;
                        self.repo_ctx.issue_body_scroll = 0;
                        self.repo_ctx.repo_frontpage_scroll = 0;
                        self.dispatch_repo_view_trigger();
                    }
                }
                ReposView::PrList => {
                    if self
                        .source_ctx
                        .source_pr_state
                        .selected()
                        .and_then(|i| self.source_ctx.source_prs.get(i))
                        .is_some()
                    {
                        self.focus = Column::Detail;
                        self.repo_view = crate::types::RepoView::Prs;
                        self.repo_ctx.detail_section = DetailSection::Body;
                        self.repo_ctx.pr_body_scroll = 0;
                        self.repo_ctx.check_runs_state = ListState::default();
                        self.trigger_load_pr_body();
                    }
                }
                ReposView::IssueList => {
                    if self.selected_source_issue().is_some() {
                        self.focus = Column::Detail;
                        self.repo_view = RepoView::Issues;
                        self.repo_ctx.issue_body_scroll = 0;
                        self.trigger_load_source_issue_body();
                    }
                }
            },
            Column::Repo => match self.repo_view {
                RepoView::Frontpage => {}
                RepoView::Prs => {
                    if self.selected_pr().is_some() {
                        self.focus = Column::Detail;
                        self.repo_ctx.detail_section = if self.pr_body_focusable() {
                            DetailSection::Body
                        } else if self.checks_focusable() {
                            DetailSection::Checks
                        } else {
                            DetailSection::Body
                        };
                        self.repo_ctx.pr_body_scroll = 0;
                        self.repo_ctx.check_runs_state = ListState::default();
                    }
                }
                RepoView::Issues => {
                    if self.selected_issue().is_some() {
                        self.focus = Column::Detail;
                        self.repo_ctx.issue_body_scroll = 0;
                    }
                }
            },
            Column::Detail => {}
        }
    }

    pub(crate) fn detail_tab(&mut self) {
        if self.focus != Column::Detail {
            return;
        }
        match self.repo_ctx.detail_section {
            DetailSection::Body if self.checks_focusable() => {
                self.repo_ctx.detail_section = DetailSection::Checks;
            }
            DetailSection::Checks if self.pr_body_focusable() => {
                self.repo_ctx.detail_section = DetailSection::Body;
            }
            _ => {}
        }
    }

    pub(crate) fn move_top(&mut self) {
        match self.focus {
            Column::Sources => {
                if !self.visible_sources().is_empty() && self.source_state.select_changed(Some(0)) {
                    self.on_source_changed();
                }
            }
            Column::Repos => match self.repos_view {
                ReposView::RepoList => {
                    if !self.visible_repos().is_empty()
                        && self.source_ctx.repo_state.select_changed(Some(0))
                    {
                        self.on_repo_changed();
                    }
                }
                ReposView::PrList => {
                    self.source_ctx.source_pr_state.select(Some(0));
                }
                ReposView::IssueList => {
                    self.source_ctx.source_issue_state.select(Some(0));
                }
            },
            Column::Repo => match self.repo_view {
                RepoView::Frontpage => {
                    self.repo_ctx.repo_frontpage_scroll = 0;
                }
                RepoView::Prs => {
                    if !self.repo_ctx.prs.is_empty() {
                        self.repo_ctx.pr_state.select(Some(0));
                        self.trigger_load_pr_body();
                    }
                }
                RepoView::Issues => {
                    if !self.repo_ctx.issues.is_empty() {
                        self.repo_ctx.issue_state.select(Some(0));
                        self.trigger_load_issue_body();
                    }
                }
            },
            Column::Detail => match self.repo_view {
                RepoView::Issues => {
                    self.repo_ctx.issue_body_scroll = 0;
                }
                RepoView::Prs | RepoView::Frontpage => match self.repo_ctx.detail_section {
                    DetailSection::Body => {
                        self.repo_ctx.pr_body_scroll = 0;
                    }
                    DetailSection::Checks => {
                        self.repo_ctx.check_runs_state.select(Some(0));
                    }
                },
            },
        }
    }

    pub(crate) fn move_bottom(&mut self) {
        match self.focus {
            Column::Sources => {
                let len = self.visible_sources().len();
                if len > 0 && self.source_state.select_changed(Some(len - 1)) {
                    self.on_source_changed();
                }
            }
            Column::Repos => match self.repos_view {
                ReposView::RepoList => {
                    let len = self.visible_repos().len();
                    if len > 0 && self.source_ctx.repo_state.select_changed(Some(len - 1)) {
                        self.on_repo_changed();
                    }
                }
                ReposView::PrList => {
                    let len = self.source_ctx.source_prs.len();
                    if len > 0 {
                        self.source_ctx.source_pr_state.select(Some(len - 1));
                    }
                }
                ReposView::IssueList => {
                    let len = self.source_ctx.source_issues.len();
                    if len > 0 {
                        self.source_ctx.source_issue_state.select(Some(len - 1));
                    }
                }
            },
            Column::Repo => match self.repo_view {
                RepoView::Frontpage => {
                    self.repo_ctx.repo_frontpage_scroll = u16::MAX;
                }
                RepoView::Prs => {
                    if !self.repo_ctx.prs.is_empty() {
                        self.repo_ctx
                            .pr_state
                            .select(Some(self.repo_ctx.prs.len() - 1));
                        self.trigger_load_pr_body();
                    }
                }
                RepoView::Issues => {
                    if !self.repo_ctx.issues.is_empty() {
                        self.repo_ctx
                            .issue_state
                            .select(Some(self.repo_ctx.issues.len() - 1));
                        self.trigger_load_issue_body();
                    }
                }
            },
            Column::Detail => match self.repo_view {
                RepoView::Issues => {
                    self.repo_ctx.issue_body_scroll = u16::MAX;
                }
                RepoView::Prs | RepoView::Frontpage => match self.repo_ctx.detail_section {
                    DetailSection::Body => {
                        self.repo_ctx.pr_body_scroll = u16::MAX;
                    }
                    DetailSection::Checks => {
                        let len = self.repo_ctx.check_runs.as_ref().map_or(0, Vec::len);
                        if len > 0 {
                            self.repo_ctx.check_runs_state.select(Some(len - 1));
                        }
                    }
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_prev_from_zero_stays_at_zero() {
        let mut state = ListState::default();
        state.select(Some(0));
        let changed = state.nav_prev(3);
        assert_eq!(state.selected(), Some(0));
        assert!(!changed);
    }

    #[test]
    fn nav_prev_from_middle_decrements() {
        let mut state = ListState::default();
        state.select(Some(2));
        let changed = state.nav_prev(3);
        assert_eq!(state.selected(), Some(1));
        assert!(changed);
    }

    #[test]
    fn nav_next_from_last_stays() {
        let mut state = ListState::default();
        state.select(Some(2));
        let changed = state.nav_next(3);
        assert_eq!(state.selected(), Some(2));
        assert!(!changed);
    }

    #[test]
    fn nav_next_from_middle_increments() {
        let mut state = ListState::default();
        state.select(Some(1));
        let changed = state.nav_next(3);
        assert_eq!(state.selected(), Some(2));
        assert!(changed);
    }

    #[test]
    fn nav_prev_empty_list_returns_false() {
        let mut state = ListState::default();
        state.select(Some(0));
        let changed = state.nav_prev(0);
        assert!(!changed);
    }

    #[test]
    fn nav_next_empty_list_returns_false() {
        let mut state = ListState::default();
        state.select(Some(0));
        let changed = state.nav_next(0);
        assert!(!changed);
    }

    #[test]
    fn nav_next_no_selection_selects_zero() {
        let mut state = ListState::default();
        let changed = state.nav_next(3);
        assert_eq!(state.selected(), Some(0));
        assert!(changed);
    }

    #[test]
    fn nav_prev_no_selection_selects_zero() {
        let mut state = ListState::default();
        let changed = state.nav_prev(3);
        assert_eq!(state.selected(), Some(0));
        assert!(changed);
    }

    #[test]
    fn select_changed_same_returns_false() {
        let mut state = ListState::default();
        state.select(Some(1));
        let changed = state.select_changed(Some(1));
        assert!(!changed);
    }

    #[test]
    fn select_changed_different_returns_true() {
        let mut state = ListState::default();
        state.select(Some(1));
        let changed = state.select_changed(Some(2));
        assert!(changed);
    }

    #[test]
    fn clamp_list_state_clamps_past_end() {
        let mut state = ListState::default();
        state.select(Some(5));
        clamp_list_state(&mut state, 3);
        assert_eq!(state.selected(), Some(2));
    }

    #[test]
    fn clamp_list_state_empty_clears() {
        let mut state = ListState::default();
        state.select(Some(0));
        clamp_list_state(&mut state, 0);
        assert_eq!(state.selected(), None);
    }
}
