use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    Clone,
    // PR multi-select (PR list columns only)
    ToggleSelect,
    SelectAll,
    ClearSelection,
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
    /// Modifiers required in addition to `keys`. `NONE` means any modifier state matches
    /// (the historical behaviour, before `SelectAll` needed `CONTROL`).
    pub modifiers: KeyModifiers,
}

pub static UNIVERSAL_BINDINGS: &[DefaultBinding] = &[
    DefaultBinding {
        keys: &[KeyCode::Char('q')],
        display: "q",
        action: Action::Quit,
        label: "quit",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('?')],
        display: "?",
        action: Action::Help,
        label: "help",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('R')],
        display: "R",
        action: Action::Refresh,
        label: "refresh",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('/')],
        display: "/",
        action: Action::FilterStart,
        label: "filter",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('S')],
        display: "S",
        action: Action::SortCycle,
        label: "sort",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('j'), KeyCode::Down],
        display: "j/↓",
        action: Action::Down,
        label: "move down",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('k'), KeyCode::Up],
        display: "k/↑",
        action: Action::Up,
        label: "move up",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('h'), KeyCode::Left],
        display: "h/←",
        action: Action::Left,
        label: "focus left",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('l'), KeyCode::Right, KeyCode::Enter],
        display: "l/→",
        action: Action::Right,
        label: "focus right",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('g'), KeyCode::Home],
        display: "g",
        action: Action::Top,
        label: "jump to top",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('G'), KeyCode::End],
        display: "G",
        action: Action::Bottom,
        label: "jump to bottom",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('o')],
        display: "o",
        action: Action::OpenBrowser,
        label: "open",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('i')],
        display: "i",
        action: Action::OpenIssues,
        label: "issues",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('y')],
        display: "y",
        action: Action::CopyUrl,
        label: "copy URL",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('C')],
        display: "C",
        action: Action::Clone,
        label: "clone",
        modifiers: KeyModifiers::NONE,
    },
];

pub static PRS_BINDINGS: &[DefaultBinding] = &[
    DefaultBinding {
        keys: &[KeyCode::Char(' ')],
        display: "space",
        action: Action::ToggleSelect,
        label: "select",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('a')],
        display: "^a",
        action: Action::SelectAll,
        label: "select all",
        modifiers: KeyModifiers::CONTROL,
    },
    DefaultBinding {
        keys: &[KeyCode::Esc],
        display: "esc",
        action: Action::ClearSelection,
        label: "clear selection",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('v')],
        display: "v",
        action: Action::Approve,
        label: "approve",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('m')],
        display: "m",
        action: Action::Merge,
        label: "auto-merge",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('C')],
        display: "C",
        action: Action::Checkout,
        label: "checkout",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('c')],
        display: "c",
        action: Action::Comment,
        label: "comment",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('d')],
        display: "d",
        action: Action::Diff,
        label: "diff",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('x')],
        display: "x",
        action: Action::ClosePr,
        label: "close",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('X')],
        display: "X",
        action: Action::ReopenPr,
        label: "reopen",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('W')],
        display: "W",
        action: Action::MarkReady,
        label: "mark ready",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('b')],
        display: "b",
        action: Action::DependabotMenu,
        label: "dependabot",
        modifiers: KeyModifiers::NONE,
    },
];

pub static ISSUES_BINDINGS: &[DefaultBinding] = &[];

pub static REPOS_BINDINGS: &[DefaultBinding] = &[
    DefaultBinding {
        keys: &[KeyCode::Char('r')],
        display: "r",
        action: Action::ViewRepos,
        label: "repos",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('p')],
        display: "p",
        action: Action::ViewPrs,
        label: "prs",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('i')],
        display: "i",
        action: Action::ViewIssues,
        label: "issues",
        modifiers: KeyModifiers::NONE,
    },
];

pub static CHECKS_BINDINGS: &[DefaultBinding] = &[
    DefaultBinding {
        keys: &[KeyCode::Char('o')],
        display: "o",
        action: Action::CheckOpen,
        label: "open check",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('O')],
        display: "O",
        action: Action::OpenBrowser,
        label: "open PR",
        modifiers: KeyModifiers::NONE,
    },
    DefaultBinding {
        keys: &[KeyCode::Char('R')],
        display: "R",
        action: Action::CheckRerun,
        label: "re-run",
        modifiers: KeyModifiers::NONE,
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

/// A user keybinding that shadows a built-in binding for the same key. Because user
/// bindings are matched before the defaults of their own layer and every later layer, a
/// custom key silently wins over any built-in on the same key in those positions. This
/// records which built-ins are hidden so they can be surfaced to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clobber {
    /// Scope the user binding lives in ("universal", "prs", ...).
    pub scope: &'static str,
    /// The user's key string as written in the config.
    pub key: String,
    /// The built-in action that is shadowed.
    pub action: Action,
}

impl Clobber {
    /// Display key and label of the clobbered built-in, via `find_binding`.
    pub fn builtin_display(&self) -> Option<(&'static str, &'static str)> {
        find_binding(self.action).map(|b| (b.display, b.label))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Scope {
    Universal,
    Repos,
    Prs,
    Issues,
    Checks,
}

impl Scope {
    const fn name(self) -> &'static str {
        match self {
            Scope::Universal => "universal",
            Scope::Repos => "repos",
            Scope::Prs => "prs",
            Scope::Issues => "issues",
            Scope::Checks => "checks",
        }
    }

    const fn table(self) -> &'static [DefaultBinding] {
        match self {
            Scope::Universal => UNIVERSAL_BINDINGS,
            Scope::Repos => REPOS_BINDINGS,
            Scope::Prs => PRS_BINDINGS,
            Scope::Issues => ISSUES_BINDINGS,
            Scope::Checks => CHECKS_BINDINGS,
        }
    }
}

struct LayerSpec {
    scope: Scope,
    /// Whether user keybindings in this scope are checked for this layer. The view-switch
    /// layers that back the Browse column while a PR/issue list is open expose only their
    /// defaults (no user bindings), so `user_active` is false there.
    user_active: bool,
}

const fn layer(scope: Scope, user_active: bool) -> LayerSpec {
    LayerSpec { scope, user_active }
}

/// The ordered key layers for every input context. Must stay in sync with
/// `active_layers()` in app/event_loop.rs: same scopes, same order.
const CONTEXTS: &[&[LayerSpec]] = &[
    // Checks section of the detail panel (also inherits PR keys).
    &[
        layer(Scope::Checks, true),
        layer(Scope::Prs, true),
        layer(Scope::Universal, true),
    ],
    // PR list while the Browse column is focused (view-switch layer, defaults only).
    &[
        layer(Scope::Repos, false),
        layer(Scope::Prs, true),
        layer(Scope::Universal, true),
    ],
    // PR list / source PR list / PR detail.
    &[layer(Scope::Prs, true), layer(Scope::Universal, true)],
    // Issue list while the Browse column is focused (view-switch layer, defaults only).
    &[
        layer(Scope::Repos, false),
        layer(Scope::Issues, true),
        layer(Scope::Universal, true),
    ],
    // Issue list / issue detail.
    &[layer(Scope::Issues, true), layer(Scope::Universal, true)],
    // Repos column (repo list).
    &[layer(Scope::Repos, true), layer(Scope::Universal, true)],
    // Sources / frontpage.
    &[layer(Scope::Universal, true)],
];

fn key_matches(code: KeyCode, binding_keys: &[KeyCode]) -> bool {
    // Built-in dispatch matches on `KeyCode` first (see find_layer_match), so clobber
    // detection compares the same way to agree with what actually wins. Dropping the
    // custom binding's own modifiers (see `binding_code`) is a safe over-approximation:
    // a user binding with fewer required modifiers than a built-in (e.g. plain `a`, which
    // requires none) always fires whenever the built-in would (e.g. `ctrl+a`), so treating
    // any same-code binding as a potential clobber never under-reports. It can only ever
    // over-report for a binding whose modifiers are unrelated to the built-in's (e.g.
    // `alt+a` against `ctrl+a` select-all) - an accepted, rare false positive.
    binding_keys.contains(&code)
}

/// Parsed `KeyCode` for a keybinding, or `None` if the key string is malformed.
/// Modifiers are dropped for clobber-detection purposes; see `key_matches`.
fn binding_code(kb: &crate::config::Keybinding) -> Option<KeyCode> {
    crate::config::parse_key(&kb.key).map(|(c, _)| c)
}

/// Find the built-in bindings that a user's keybindings shadow. A user binding is checked
/// before the defaults of its own layer and every later layer, so it wins over any built-in
/// on the same key in those positions (across every input context where it is active).
///
/// The simulation mirrors `find_layer_match` exactly: per layer, user bindings in config
/// order first, then that layer's defaults; the first match wins and stops dispatch. A
/// binding is only reported if it actually fires (no earlier user binding or default in an
/// earlier layer takes the key first).
///
/// Returns one `Clobber` per (scope, key, action). Re-mapping an action to itself
/// (`builtin = "<same action>"`) is a behaviour-preserving no-op and is not reported.
pub fn clobbered_bindings(cfg: &crate::config::KeybindingsConfig) -> Vec<Clobber> {
    let scopes: [(Scope, &[crate::config::Keybinding]); 5] = [
        (Scope::Universal, &cfg.universal),
        (Scope::Repos, &cfg.repos),
        (Scope::Prs, &cfg.prs),
        (Scope::Issues, &cfg.issues),
        (Scope::Checks, &cfg.checks),
    ];

    let user_kbs = |scope: Scope| -> &[crate::config::Keybinding] {
        scopes
            .iter()
            .find(|(s, _)| *s == scope)
            .map(|(_, kbs)| *kbs)
            .unwrap_or(&[])
    };

    let mut out: Vec<Clobber> = Vec::new();
    let mut seen: std::collections::HashSet<(Scope, String, Action)> =
        std::collections::HashSet::new();

    for (scope, kbs) in scopes {
        for (idx, kb) in kbs.iter().enumerate() {
            let Some(code) = binding_code(kb) else {
                continue;
            };
            // A `builtin` that resolves to the same action is a no-op re-map, not a clobber.
            let self_action = kb.builtin.as_deref().and_then(builtin_to_action);

            // User bindings of the same scope that come before this one in config order.
            let earlier_same_scope = kbs[..idx].iter().any(|k| binding_code(k) == Some(code));

            let mut clobbered: std::collections::HashSet<Action> = std::collections::HashSet::new();
            for layers in CONTEXTS {
                // Our user binding is only checked where this scope's layer has a user section.
                let Some(i) = layers
                    .iter()
                    .position(|l| l.scope == scope && l.user_active)
                else {
                    continue;
                };
                // Anything checked before our binding that matches the key wins first, so we
                // never fire in this context: an earlier same-scope binding, or any user
                // binding / default of an earlier layer.
                let shadowed = earlier_same_scope
                    || layers[..i].iter().any(|l| {
                        (l.user_active
                            && user_kbs(l.scope)
                                .iter()
                                .any(|k| binding_code(k) == Some(code)))
                            || l.scope.table().iter().any(|b| key_matches(code, b.keys))
                    });
                if shadowed {
                    continue;
                }
                // Our binding fires here; it shadows built-ins in its own layer and all later ones.
                for l in &layers[i..] {
                    for b in l.scope.table() {
                        if key_matches(code, b.keys) {
                            clobbered.insert(b.action);
                        }
                    }
                }
            }

            for action in clobbered {
                if Some(action) == self_action {
                    continue; // re-mapped to itself: no behaviour change
                }
                if seen.insert((scope, kb.key.clone(), action)) {
                    out.push(Clobber {
                        scope: scope.name(),
                        key: kb.key.clone(),
                        action,
                    });
                }
            }
        }
    }

    out
}

/// One-line, human-readable summary of clobbered built-ins for the status bar.
/// Empty when there is nothing to report.
pub fn clobber_summary(cfg: &crate::config::KeybindingsConfig) -> String {
    let clobbers = clobbered_bindings(cfg);
    if clobbers.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = clobbers
        .iter()
        .map(|c| {
            let (display, label) = c.builtin_display().unwrap_or(("", ""));
            format!("{} ({}) shadows {} {}", c.key, c.scope, display, label)
        })
        .collect();
    // Keep the status line short: at most three examples, then a count.
    if parts.len() > 3 {
        let total = parts.len();
        parts.truncate(3);
        parts.push(format!("+{} more", total - 3));
    }
    format!("keybinding clobber: {} (see ?)", parts.join("; "))
}

/// Which actions to show in the status-bar hint for each column.
pub const SOURCES_BAR: &[Action] = &[
    Action::OpenBrowser,
    Action::CopyUrl,
    Action::Clone,
    Action::FilterStart,
];

pub const REPOS_BAR: &[Action] = &[
    Action::ViewRepos,
    Action::ViewPrs,
    Action::ViewIssues,
    Action::OpenBrowser,
    Action::CopyUrl,
    Action::Clone,
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
    Action::ToggleSelect,
    Action::SelectAll,
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
    Action::ToggleSelect,
    Action::SelectAll,
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

pub const FRONTPAGE_BAR: &[Action] = &[Action::OpenBrowser, Action::CopyUrl, Action::Clone];

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
        "clone" => Some(Action::Clone),
        "toggleSelect" => Some(Action::ToggleSelect),
        "selectAll" => Some(Action::SelectAll),
        "clearSelection" => Some(Action::ClearSelection),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Keybinding, KeybindingsConfig};

    fn kb(key: &str) -> Keybinding {
        Keybinding {
            key: key.into(),
            name: None,
            builtin: None,
            command: Some("true".into()),
            interactive: false,
        }
    }

    fn kb_builtin(key: &str, builtin: &str) -> Keybinding {
        Keybinding {
            key: key.into(),
            name: None,
            builtin: Some(builtin.into()),
            command: None,
            interactive: false,
        }
    }

    fn cfg(
        kb_universal: Vec<Keybinding>,
        kb_repos: Vec<Keybinding>,
        kb_prs: Vec<Keybinding>,
    ) -> KeybindingsConfig {
        KeybindingsConfig {
            universal: kb_universal,
            repos: kb_repos,
            prs: kb_prs,
            ..KeybindingsConfig::default()
        }
    }

    fn actions(c: &[Clobber]) -> Vec<Action> {
        c.iter().map(|c| c.action).collect()
    }

    #[test]
    fn empty_config_has_no_clobbers() {
        assert!(clobbered_bindings(&KeybindingsConfig::default()).is_empty());
    }

    #[test]
    fn prs_command_on_a_clobbers_select_all() {
        // The reported bug: a custom `a` in [keybindings.prs] hides the built-in
        // select-all (ctrl+a) from multi-PR selection.
        let c = clobbered_bindings(&cfg(vec![], vec![], vec![kb("a")]));
        assert_eq!(actions(&c), vec![Action::SelectAll]);
        let clobber = &c[0];
        assert_eq!(clobber.scope, "prs");
        assert_eq!(clobber.key, "a");
        assert_eq!(clobber.builtin_display(), Some(("^a", "select all")));
    }

    #[test]
    fn universal_command_on_a_does_not_clobber_select_all() {
        // In the PR context the PRs layer is checked before the universal one, and its
        // defaults already bind `a` to select all. So a *universal* `a` is itself shadowed
        // there and never fires; it does not clobber select all. (Only a prs-scope `a`
        // would, since prs user bindings are checked before prs defaults.)
        let c = clobbered_bindings(&cfg(vec![kb("a")], vec![], vec![]));
        assert!(c.is_empty());
    }

    #[test]
    fn universal_command_on_o_clobbers_open_browser() {
        // `o` is a universal built-in with no earlier layer claiming it, so a universal
        // command on `o` clobbers open browser everywhere.
        let c = clobbered_bindings(&cfg(vec![kb("o")], vec![], vec![]));
        assert_eq!(actions(&c), vec![Action::OpenBrowser]);
    }

    #[test]
    fn ctrl_a_clobbers_select_all() {
        // A `ctrl+a` custom binding is the literal same combo as the built-in, so it
        // clobbers it (unlike unrelated modifiers, e.g. `alt+a`, which built-in matching
        // by KeyCode alone can't distinguish from this - an accepted over-approximation,
        // see `key_matches`).
        let c = clobbered_bindings(&cfg(vec![], vec![], vec![kb("ctrl+a")]));
        assert_eq!(actions(&c), vec![Action::SelectAll]);
    }

    #[test]
    fn ctrl_key_clobbers_nothing_without_matching_builtin() {
        // A *universal* `ctrl+a` is shadowed by the PRs-layer select-all default wherever
        // the PRs layer is active, and no other built-in uses code 'a', so it never
        // actually clobbers anything.
        let c = clobbered_bindings(&cfg(vec![kb("ctrl+a")], vec![], vec![]));
        assert!(c.is_empty());
    }

    #[test]
    fn builtin_remap_to_same_action_is_not_a_clobber() {
        // `a -> selectAll` keeps the behaviour; it is a documented override, not a clobber.
        let c = clobbered_bindings(&cfg(vec![], vec![], vec![kb_builtin("a", "selectAll")]));
        assert!(c.is_empty());
    }

    #[test]
    fn builtin_remap_to_other_action_is_a_clobber() {
        // `a -> merge` makes select-all unreachable.
        let c = clobbered_bindings(&cfg(vec![], vec![], vec![kb_builtin("a", "merge")]));
        assert_eq!(actions(&c), vec![Action::SelectAll]);
    }

    #[test]
    fn later_same_scope_binding_is_not_reported_when_itself_shadowed() {
        // First `a` (command) wins; the second `a` (builtin merge) is never reached, so it
        // must not be reported as clobbering anything.
        let c = clobbered_bindings(&cfg(
            vec![],
            vec![],
            vec![kb("a"), kb_builtin("a", "merge")],
        ));
        assert_eq!(actions(&c), vec![Action::SelectAll]);
        assert!(
            c.iter()
                .all(|x| x.key == "a" && x.action == Action::SelectAll)
        );
    }

    #[test]
    fn prs_binding_r_clobbers_nothing() {
        // `r` is a built-in only in the repos layer (ViewRepos). In the context where that
        // built-in is active, the repos defaults are checked before prs user bindings, so a
        // prs `r` is itself shadowed there; elsewhere it fires but clobbers nothing.
        let c = clobbered_bindings(&cfg(vec![], vec![], vec![kb("r")]));
        assert!(c.is_empty());
    }

    #[test]
    fn prs_command_on_o_clobbers_open_browser() {
        // `o` is a universal built-in (OpenBrowser); prs user bindings are checked first.
        let c = clobbered_bindings(&cfg(vec![], vec![], vec![kb("o")]));
        assert_eq!(actions(&c), vec![Action::OpenBrowser]);
    }

    #[test]
    fn clobber_summary_empty_when_no_clobbers() {
        assert_eq!(clobber_summary(&KeybindingsConfig::default()), "");
    }

    #[test]
    fn clobber_summary_lists_shadowed_builtins() {
        let s = clobber_summary(&cfg(vec![], vec![], vec![kb("a")]));
        assert!(s.starts_with("keybinding clobber:"));
        assert!(s.contains("a (prs) shadows ^a select all"), "got: {s}");
        assert!(s.contains("see ?"), "got: {s}");
    }

    #[test]
    fn clobber_summary_caps_examples_at_three() {
        let c = clobbered_bindings(&cfg(
            vec![],
            vec![],
            vec![kb("a"), kb("v"), kb("m"), kb("x")],
        ));
        assert_eq!(c.len(), 4);
        let s = clobber_summary(&cfg(
            vec![],
            vec![],
            vec![kb("a"), kb("v"), kb("m"), kb("x")],
        ));
        assert!(s.contains("+1 more"), "got: {s}");
    }
}
