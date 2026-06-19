use crossterm::event::{KeyCode, KeyEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    Top,
    Bottom,
    Quit,
    Help,
    Refresh,
    FilterStart,
    SortCycle,
    // Browse column view switching
    ViewRepos,
    ViewPrs,
    ViewIssues,
    // Context-sensitive (behaviour varies by focused column)
    OpenBrowser,
    OpenIssues,
    CopyUrl,
    // PR-only actions
    Approve,
    Merge,
    Checkout,
    Comment,
    Diff,
    ClosePr,
    ReopenPr,
    MarkReady,
    DependabotMenu,
    CheckOpen,
    CheckRerun,
}

pub struct DefaultBinding {
    pub keys: &'static [KeyCode],
    pub display: &'static str,
    pub action: Action,
    pub label: &'static str,
}

pub static UNIVERSAL_BINDINGS: &[DefaultBinding] = &[
    DefaultBinding {
        keys: &[KeyCode::Char('q')],
        display: "q",
        action: Action::Quit,
        label: "quit",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('?')],
        display: "?",
        action: Action::Help,
        label: "help",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('R')],
        display: "R",
        action: Action::Refresh,
        label: "refresh",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('/')],
        display: "/",
        action: Action::FilterStart,
        label: "filter",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('S')],
        display: "S",
        action: Action::SortCycle,
        label: "sort",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('j'), KeyCode::Down],
        display: "j/↓",
        action: Action::Down,
        label: "move down",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('k'), KeyCode::Up],
        display: "k/↑",
        action: Action::Up,
        label: "move up",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('h'), KeyCode::Left],
        display: "h/←",
        action: Action::Left,
        label: "focus left",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('l'), KeyCode::Right, KeyCode::Enter],
        display: "l/→",
        action: Action::Right,
        label: "focus right",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('g'), KeyCode::Home],
        display: "g",
        action: Action::Top,
        label: "jump to top",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('G'), KeyCode::End],
        display: "G",
        action: Action::Bottom,
        label: "jump to bottom",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('o')],
        display: "o",
        action: Action::OpenBrowser,
        label: "open",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('i')],
        display: "i",
        action: Action::OpenIssues,
        label: "issues",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('y')],
        display: "y",
        action: Action::CopyUrl,
        label: "copy URL",
    },
];

pub static PRS_BINDINGS: &[DefaultBinding] = &[
    DefaultBinding {
        keys: &[KeyCode::Char('v')],
        display: "v",
        action: Action::Approve,
        label: "approve",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('m')],
        display: "m",
        action: Action::Merge,
        label: "merge",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('C')],
        display: "C",
        action: Action::Checkout,
        label: "checkout",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('c')],
        display: "c",
        action: Action::Comment,
        label: "comment",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('d')],
        display: "d",
        action: Action::Diff,
        label: "diff",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('x')],
        display: "x",
        action: Action::ClosePr,
        label: "close",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('X')],
        display: "X",
        action: Action::ReopenPr,
        label: "reopen",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('W')],
        display: "W",
        action: Action::MarkReady,
        label: "mark ready",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('b')],
        display: "b",
        action: Action::DependabotMenu,
        label: "dependabot",
    },
];

pub static ISSUES_BINDINGS: &[DefaultBinding] = &[];

pub static REPOS_BINDINGS: &[DefaultBinding] = &[
    DefaultBinding {
        keys: &[KeyCode::Char('r')],
        display: "r",
        action: Action::ViewRepos,
        label: "repos",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('p')],
        display: "p",
        action: Action::ViewPrs,
        label: "prs",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('i')],
        display: "i",
        action: Action::ViewIssues,
        label: "issues",
    },
];

pub static CHECKS_BINDINGS: &[DefaultBinding] = &[
    DefaultBinding {
        keys: &[KeyCode::Char('o')],
        display: "o",
        action: Action::CheckOpen,
        label: "open check",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('O')],
        display: "O",
        action: Action::OpenBrowser,
        label: "open PR",
    },
    DefaultBinding {
        keys: &[KeyCode::Char('R')],
        display: "R",
        action: Action::CheckRerun,
        label: "re-run",
    },
];

pub const CHECKS_BAR: &[Action] = &[Action::CheckOpen, Action::CheckRerun];

// Checks section in a PR context. Checks actions listed first so 'o' = open check
// rather than open browser. OpenBrowser omitted for the same reason.
pub const CHECKS_AND_PRS_BAR: &[Action] = &[
    Action::CheckOpen,
    Action::CheckRerun,
    Action::Approve,
    Action::Merge,
    Action::Checkout,
    Action::Comment,
    Action::Diff,
    Action::CopyUrl,
    Action::FilterStart,
    Action::SortCycle,
];

/// Look up a binding by action across all tables.
pub fn find_binding(action: Action) -> Option<&'static DefaultBinding> {
    UNIVERSAL_BINDINGS
        .iter()
        .find(|b| b.action == action)
        .or_else(|| REPOS_BINDINGS.iter().find(|b| b.action == action))
        .or_else(|| PRS_BINDINGS.iter().find(|b| b.action == action))
        .or_else(|| CHECKS_BINDINGS.iter().find(|b| b.action == action))
}

/// Which actions to show in the status-bar hint for each column.
pub const SOURCES_BAR: &[Action] = &[Action::OpenBrowser, Action::CopyUrl, Action::FilterStart];

pub const REPOS_BAR: &[Action] = &[
    Action::ViewRepos,
    Action::ViewPrs,
    Action::ViewIssues,
    Action::OpenBrowser,
    Action::CopyUrl,
    Action::FilterStart,
    Action::SortCycle,
];

pub const SOURCE_ISSUES_BAR: &[Action] = &[
    Action::ViewRepos,
    Action::ViewPrs,
    Action::ViewIssues,
    Action::OpenBrowser,
    Action::CopyUrl,
    Action::FilterStart,
];

pub const SOURCE_PRS_BAR: &[Action] = &[
    Action::ViewRepos,
    Action::ViewPrs,
    Action::ViewIssues,
    Action::Approve,
    Action::Merge,
    Action::Checkout,
    Action::Comment,
    Action::Diff,
    Action::OpenBrowser,
    Action::CopyUrl,
    Action::FilterStart,
];

pub const PRS_BAR: &[Action] = &[
    Action::Approve,
    Action::Merge,
    Action::Checkout,
    Action::Comment,
    Action::Diff,
    Action::OpenBrowser,
    Action::CopyUrl,
    Action::FilterStart,
    Action::SortCycle,
];

pub const ISSUES_BAR: &[Action] = &[
    Action::OpenBrowser,
    Action::CopyUrl,
    Action::FilterStart,
    Action::SortCycle,
];

pub const FRONTPAGE_BAR: &[Action] = &[Action::OpenBrowser, Action::CopyUrl];

/// Navigation actions shown in the help popup navigation section.
pub const NAV_ACTIONS: &[Action] = &[
    Action::Down,
    Action::Up,
    Action::Left,
    Action::Right,
    Action::Top,
    Action::Bottom,
    Action::Refresh,
    Action::Help,
    Action::Quit,
];

/// Diff-view status-bar hint.
pub const DIFF_HINT_TEXT: &str = "j/k scroll  g/G top/bottom  h/q close";

pub fn builtin_to_action(name: &str) -> Option<Action> {
    match name {
        "up" => Some(Action::Up),
        "down" => Some(Action::Down),
        "left" => Some(Action::Left),
        "right" | "enter" => Some(Action::Right),
        "top" | "firstLine" => Some(Action::Top),
        "bottom" | "lastLine" => Some(Action::Bottom),
        "quit" => Some(Action::Quit),
        "help" => Some(Action::Help),
        "refresh" => Some(Action::Refresh),
        "filter" | "search" => Some(Action::FilterStart),
        "sort" => Some(Action::SortCycle),
        "viewRepos" => Some(Action::ViewRepos),
        "viewPrs" => Some(Action::ViewPrs),
        "viewIssues" => Some(Action::ViewIssues),
        "openBrowser" | "openGithub" => Some(Action::OpenBrowser),
        "openIssues" => Some(Action::OpenIssues),
        "copyUrl" => Some(Action::CopyUrl),
        "approve" => Some(Action::Approve),
        "merge" => Some(Action::Merge),
        "checkout" => Some(Action::Checkout),
        "comment" => Some(Action::Comment),
        "diff" => Some(Action::Diff),
        "close" => Some(Action::ClosePr),
        "reopen" => Some(Action::ReopenPr),
        "ready" | "markReady" => Some(Action::MarkReady),
        "dependabot" => Some(Action::DependabotMenu),
        "checkOpen" => Some(Action::CheckOpen),
        "checkRerun" => Some(Action::CheckRerun),
        _ => None,
    }
}

pub fn map_key_universal(key: KeyEvent) -> Option<Action> {
    UNIVERSAL_BINDINGS
        .iter()
        .find(|b| b.keys.contains(&key.code))
        .map(|b| b.action)
}
