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

impl App {
    pub(crate) fn clamp_source_selection(&mut self) {
        let len = self.visible_sources().len();
        clamp_list_state(&mut self.source_state, len);
    }

    pub(crate) fn clamp_repo_selection(&mut self) {
        let len = self.visible_repos().len();
        clamp_list_state(&mut self.repo_state, len);
    }

    pub(crate) fn move_up(&mut self) {
        match self.focus {
            Column::Sources => {
                let len = self.visible_sources().len();
                if select_prev(&mut self.source_state, len) {
                    self.trigger_load_repos();
                }
            }
            Column::Repos => {
                let len = self.visible_repos().len();
                if select_prev(&mut self.repo_state, len) {
                    self.trigger_load_prs();
                }
            }
            Column::Repo => match self.repo_view {
                RepoView::Frontpage => {
                    self.repo_frontpage_scroll = self.repo_frontpage_scroll.saturating_sub(1);
                }
                RepoView::Prs => {
                    if select_prev(&mut self.pr_state, self.prs.len()) {
                        self.trigger_load_pr_body();
                    }
                }
                RepoView::Issues => {
                    if select_prev(&mut self.issue_state, self.issues.len()) {
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
                        select_prev(&mut self.check_runs_state, len);
                    }
                },
            },
        }
    }

    pub(crate) fn move_down(&mut self) {
        match self.focus {
            Column::Sources => {
                let len = self.visible_sources().len();
                if select_next(&mut self.source_state, len) {
                    self.trigger_load_repos();
                }
            }
            Column::Repos => {
                let len = self.visible_repos().len();
                let at_last = len > 0 && self.repo_state.selected() == Some(len - 1);
                if select_next(&mut self.repo_state, len) {
                    self.trigger_load_prs();
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
                    if select_next(&mut self.pr_state, len) {
                        self.trigger_load_pr_body();
                    }
                    if at_last && self.pr_filter.is_empty() {
                        self.trigger_load_more_prs();
                    }
                }
                RepoView::Issues => {
                    let len = self.issues.len();
                    let at_last = len > 0 && self.issue_state.selected() == Some(len - 1);
                    if select_next(&mut self.issue_state, len) {
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
                        select_next(&mut self.check_runs_state, len);
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
                if !self.visible_sources().is_empty() {
                    self.source_state.select(Some(0));
                    self.trigger_load_repos();
                }
            }
            Column::Repos => {
                if !self.visible_repos().is_empty() {
                    self.repo_state.select(Some(0));
                    self.trigger_load_prs();
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
                if len > 0 {
                    self.source_state.select(Some(len - 1));
                    self.trigger_load_repos();
                }
            }
            Column::Repos => {
                let len = self.visible_repos().len();
                if len > 0 {
                    self.repo_state.select(Some(len - 1));
                    self.trigger_load_prs();
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

fn select_next(state: &mut ListState, len: usize) -> bool {
    if len == 0 {
        return false;
    }
    let next = state.selected().map_or(0, |i| (i + 1).min(len - 1));
    let changed = state.selected() != Some(next);
    state.select(Some(next));
    changed
}

fn select_prev(state: &mut ListState, len: usize) -> bool {
    if len == 0 {
        return false;
    }
    let prev = state.selected().map_or(0, |i| i.saturating_sub(1));
    let changed = state.selected() != Some(prev);
    state.select(Some(prev));
    changed
}
