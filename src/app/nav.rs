use super::App;
use crate::types::{Column, DetailSection, RepoView};
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
        clamp_list_state(&mut self.repo_state, len);
    }

    fn on_source_changed(&mut self) {
        self.repo_filter.clear();
        self.pr_filter.clear();
        self.repo_state = ListState::default();
        self.pr_state = ListState::default();
        self.issue_state = ListState::default();
        self.trigger_load_repos();
    }

    fn on_repo_changed(&mut self) {
        self.pr_filter.clear();
        self.pr_state = ListState::default();
        self.issue_state = ListState::default();
        self.trigger_load_prs();
    }

    pub(crate) fn move_up(&mut self) {
        match self.focus {
            Column::Sources => {
                let len = self.visible_sources().len();
                if self.source_state.nav_prev(len) {
                    self.on_source_changed();
                }
            }
            Column::Repos => {
                let len = self.visible_repos().len();
                if self.repo_state.nav_prev(len) {
                    self.on_repo_changed();
                }
            }
            Column::Repo => match self.repo_view {
                RepoView::Frontpage => {
                    self.repo_frontpage_scroll = self.repo_frontpage_scroll.saturating_sub(1);
                }
                RepoView::Prs => {
                    if self.pr_state.nav_prev(self.prs.len()) {
                        self.trigger_load_pr_body();
                    }
                }
                RepoView::Issues => {
                    if self.issue_state.nav_prev(self.issues.len()) {
                        self.trigger_load_issue_body();
                    }
                }
            },
            Column::Detail => match self.repo_view {
                RepoView::Issues => {
                    self.issue_body_scroll = self.issue_body_scroll.saturating_sub(1);
                }
                _ => match self.detail_section {
                    DetailSection::Body => {
                        self.pr_body_scroll = self.pr_body_scroll.saturating_sub(1);
                    }
                    DetailSection::Checks => {
                        let len = self.check_runs.as_ref().map_or(0, Vec::len);
                        self.check_runs_state.nav_prev(len);
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
            Column::Repos => {
                let len = self.visible_repos().len();
                let at_last = len > 0 && self.repo_state.selected() == Some(len - 1);
                if self.repo_state.nav_next(len) {
                    self.on_repo_changed();
                }
                if at_last && self.repo_filter.is_empty() {
                    self.trigger_load_more_repos();
                }
            }
            Column::Repo => match self.repo_view {
                RepoView::Frontpage => {
                    self.repo_frontpage_scroll = self.repo_frontpage_scroll.saturating_add(1);
                }
                RepoView::Prs => {
                    let len = self.prs.len();
                    let at_last = len > 0 && self.pr_state.selected() == Some(len - 1);
                    if self.pr_state.nav_next(len) {
                        self.trigger_load_pr_body();
                    }
                    if at_last && self.pr_filter.is_empty() {
                        self.trigger_load_more_prs();
                    }
                }
                RepoView::Issues => {
                    let len = self.issues.len();
                    let at_last = len > 0 && self.issue_state.selected() == Some(len - 1);
                    if self.issue_state.nav_next(len) {
                        self.trigger_load_issue_body();
                    }
                    if at_last {
                        self.trigger_load_more_issues();
                    }
                }
            },
            Column::Detail => match self.repo_view {
                RepoView::Issues => {
                    self.issue_body_scroll = self.issue_body_scroll.saturating_add(1);
                }
                _ => match self.detail_section {
                    DetailSection::Body => {
                        self.pr_body_scroll = self.pr_body_scroll.saturating_add(1);
                    }
                    DetailSection::Checks => {
                        let len = self.check_runs.as_ref().map_or(0, Vec::len);
                        self.check_runs_state.nav_next(len);
                    }
                },
            },
        }
    }

    pub(crate) fn move_left(&mut self) {
        match self.focus {
            Column::Repos => self.focus = Column::Sources,
            Column::Repo => self.focus = Column::Repos,
            Column::Detail => self.focus = Column::Repo,
            Column::Sources => {}
        }
    }

    pub(crate) fn move_right(&mut self) {
        match self.focus {
            Column::Sources => {
                if self.selected_source().is_some() {
                    self.focus = Column::Repos;
                    if self.repos.is_empty() {
                        self.trigger_load_repos();
                    }
                }
            }
            Column::Repos => {
                if self.selected_repo().is_some() {
                    self.focus = Column::Repo;
                    self.repo_view = self.config.ui.default_repo_view;
                    self.pr_body_scroll = 0;
                    self.issue_body_scroll = 0;
                    self.repo_frontpage_scroll = 0;
                    self.dispatch_repo_view_trigger();
                }
            }
            Column::Repo => match self.repo_view {
                RepoView::Frontpage => {}
                RepoView::Prs => {
                    if self.selected_pr().is_some() {
                        self.focus = Column::Detail;
                        self.detail_section = if self.pr_body_focusable() {
                            DetailSection::Body
                        } else if self.checks_focusable() {
                            DetailSection::Checks
                        } else {
                            DetailSection::Body
                        };
                        self.pr_body_scroll = 0;
                        self.check_runs_state = ListState::default();
                    }
                }
                RepoView::Issues => {
                    if self.selected_issue().is_some() {
                        self.focus = Column::Detail;
                        self.issue_body_scroll = 0;
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
        self.detail_section = match self.detail_section {
            DetailSection::Body if self.checks_focusable() => DetailSection::Checks,
            DetailSection::Body => DetailSection::Body,
            DetailSection::Checks if self.pr_body_focusable() => DetailSection::Body,
            DetailSection::Checks => DetailSection::Checks,
        };
    }

    pub(crate) fn move_top(&mut self) {
        match self.focus {
            Column::Sources => {
                if !self.visible_sources().is_empty() && self.source_state.select_changed(Some(0)) {
                    self.on_source_changed();
                }
            }
            Column::Repos => {
                if !self.visible_repos().is_empty() && self.repo_state.select_changed(Some(0)) {
                    self.on_repo_changed();
                }
            }
            Column::Repo => match self.repo_view {
                RepoView::Frontpage => {
                    self.repo_frontpage_scroll = 0;
                }
                RepoView::Prs => {
                    if !self.prs.is_empty() {
                        self.pr_state.select(Some(0));
                        self.trigger_load_pr_body();
                    }
                }
                RepoView::Issues => {
                    if !self.issues.is_empty() {
                        self.issue_state.select(Some(0));
                        self.trigger_load_issue_body();
                    }
                }
            },
            Column::Detail => match self.repo_view {
                RepoView::Issues => {
                    self.issue_body_scroll = 0;
                }
                _ => match self.detail_section {
                    DetailSection::Body => {
                        self.pr_body_scroll = 0;
                    }
                    DetailSection::Checks => {
                        self.check_runs_state.select(Some(0));
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
            Column::Repos => {
                let len = self.visible_repos().len();
                if len > 0 && self.repo_state.select_changed(Some(len - 1)) {
                    self.on_repo_changed();
                }
            }
            Column::Repo => match self.repo_view {
                RepoView::Frontpage => {
                    self.repo_frontpage_scroll = u16::MAX;
                }
                RepoView::Prs => {
                    if !self.prs.is_empty() {
                        self.pr_state.select(Some(self.prs.len() - 1));
                        self.trigger_load_pr_body();
                    }
                }
                RepoView::Issues => {
                    if !self.issues.is_empty() {
                        self.issue_state.select(Some(self.issues.len() - 1));
                        self.trigger_load_issue_body();
                    }
                }
            },
            Column::Detail => match self.repo_view {
                RepoView::Issues => {
                    self.issue_body_scroll = u16::MAX;
                }
                _ => match self.detail_section {
                    DetailSection::Body => {
                        self.pr_body_scroll = u16::MAX;
                    }
                    DetailSection::Checks => {
                        let len = self.check_runs.as_ref().map_or(0, Vec::len);
                        if len > 0 {
                            self.check_runs_state.select(Some(len - 1));
                        }
                    }
                },
            },
        }
    }
}
