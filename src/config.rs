use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MergeMethod {
    #[default]
    Squash,
    Merge,
    Rebase,
}

impl MergeMethod {
    pub const fn flag(self) -> &'static str {
        match self {
            Self::Squash => "--squash",
            Self::Merge => "--merge",
            Self::Rebase => "--rebase",
        }
    }
}

pub const DEFAULT_TICK_MS: u64 = 100;
pub const DEFAULT_CACHE_SECS: u64 = 600;
pub const DEFAULT_RATE_LIMIT_REFRESH_SECS: u64 = 60;
pub const MIN_RATE_LIMIT_REFRESH_SECS: u64 = 10;

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub cache: CacheConfig,
    pub ui: UiConfig,
    #[serde(default)]
    pub keybindings: KeybindingsConfig,
    #[serde(default)]
    pub sources: SourcesConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    /// Seconds before a cached PR list is considered stale. Set to 0 to disable.
    pub duration_secs: u64,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// Milliseconds between UI ticks.
    pub tick_ms: u64,
    /// Default sort for the Browse column: "updated" (default), "alpha", or "created".
    pub repo_sort: crate::types::RepoSortKey,
    /// Directory to cd into before running `gh pr checkout`. Supports ~.
    pub checkout_dir: Option<String>,
    /// Extra columns shown in the repos list. Supported: "stars", "forks", "issues", "visibility", "`last_push`", "created".
    pub repo_columns: Vec<crate::types::RepoColumn>,
    /// Extra columns shown in the PR list. Supported: "`diff_stats`".
    pub pr_columns: Vec<crate::types::PrColumn>,
    /// Default view when entering a repo: "frontpage" (default), "prs", or "issues".
    pub default_repo_view: crate::types::RepoView,
    /// Default view for the Browse column: "repos" (default) or "prs".
    pub default_repos_view: crate::types::ReposView,
    /// Items per page when fetching lists. 0 = dynamic (`terminal_height` × 1.5). Max 100.
    pub per_page: u32,
    /// Merge method used by the `m` keybinding: "squash", "merge", or "rebase".
    pub merge_method: MergeMethod,
    /// Use `--auto` when merging PRs (enables auto-merge if checks haven't passed yet).
    /// Set to false to merge immediately instead.
    pub merge_auto: bool,
    /// Pre-fetch diff stats, check summary, and mergeable state for all PRs on load.
    pub prefetch_pr_details: bool,
    /// How often to refresh the rate-limit display, in seconds. Minimum 10.
    pub rate_limit_refresh_secs: u64,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct KeybindingsConfig {
    /// Keybindings active in every column. Can override defaults.
    pub universal: Vec<Keybinding>,
    /// Keybindings active when the Browse column is focused.
    pub repos: Vec<Keybinding>,
    /// Keybindings active when the PRs column is focused.
    pub prs: Vec<Keybinding>,
    /// Keybindings active when the Issues column/panel is focused.
    pub issues: Vec<Keybinding>,
    /// Keybindings active when the Checks section of the detail panel is focused.
    pub checks: Vec<Keybinding>,
}

pub trait CommandContext {
    fn expand(&self, cmd: &str) -> String;
}

pub struct PrContext<'a> {
    pub pr: &'a crate::types::PR,
    pub owner: &'a str,
    pub repo: &'a str,
}

impl CommandContext for PrContext<'_> {
    fn expand(&self, cmd: &str) -> String {
        cmd.replace("{pr_number}", &self.pr.number.to_string())
            .replace("{owner}", self.owner)
            .replace("{org}", self.owner)
            .replace("{repo}", self.repo)
            .replace("{author}", &self.pr.author)
            .replace("{head_ref}", &self.pr.head_ref)
            .replace("{base_ref}", &self.pr.base_ref)
            .replace("{url}", &self.pr.url)
            .replace("{title}", &self.pr.title)
    }
}

pub struct CheckContext<'a> {
    pub run: &'a crate::types::CheckRun,
    pub pr_number: u64,
    pub owner: &'a str,
    pub repo: &'a str,
}

impl CommandContext for CheckContext<'_> {
    fn expand(&self, cmd: &str) -> String {
        cmd.replace("{check_id}", &self.run.id.to_string())
            .replace("{check_name}", &self.run.name)
            .replace("{check_url}", &self.run.url)
            .replace("{pr_number}", &self.pr_number.to_string())
            .replace("{owner}", self.owner)
            .replace("{org}", self.owner)
            .replace("{repo}", self.repo)
    }
}

pub struct RepoContext<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub language: Option<&'a str>,
}

impl CommandContext for RepoContext<'_> {
    fn expand(&self, cmd: &str) -> String {
        let url = format!("https://github.com/{}/{}", self.owner, self.repo);
        cmd.replace("{owner}", self.owner)
            .replace("{org}", self.owner)
            .replace("{repo}", self.repo)
            .replace("{name}", self.repo)
            .replace("{language}", self.language.unwrap_or(""))
            .replace("{url}", &url)
    }
}

pub struct IssueContext<'a> {
    pub issue: &'a crate::types::Issue,
    pub owner: &'a str,
    pub repo: &'a str,
}

impl CommandContext for IssueContext<'_> {
    fn expand(&self, cmd: &str) -> String {
        cmd.replace("{issue_number}", &self.issue.number.to_string())
            .replace("{owner}", self.owner)
            .replace("{org}", self.owner)
            .replace("{repo}", self.repo)
            .replace("{author}", &self.issue.author)
            .replace("{url}", &self.issue.url)
            .replace("{title}", &self.issue.title)
    }
}

/// A single keybinding entry.
///
/// Exactly one of `builtin` or `command` should be set.
/// - `builtin`: name of a built-in action (see README for list)
/// - `command`: shell command to run; supports variable substitution
#[derive(Debug, Clone, Deserialize)]
pub struct Keybinding {
    /// Key string: single char ("s"), uppercase ("S"), "ctrl+x", or "alt+x".
    pub key: String,
    /// Display name shown in the help popup.
    #[serde(default)]
    pub name: Option<String>,
    /// Built-in action name to invoke.
    #[serde(default)]
    pub builtin: Option<String>,
    /// Shell command to run. Supports variable substitution.
    #[serde(default)]
    pub command: Option<String>,
    /// Suspend the TUI and run the command interactively (for editors, pagers, etc.).
    #[serde(default)]
    pub interactive: bool,
}

impl Keybinding {
    pub fn matches(&self, key: KeyEvent) -> bool {
        parse_key(&self.key)
            .is_some_and(|(code, mods)| key.code == code && key.modifiers.contains(mods))
    }

    pub fn expand_command<C: CommandContext>(&self, ctx: &C) -> Option<String> {
        let cmd = self.command.as_deref()?;
        Some(ctx.expand(cmd))
    }
}

/// Parse a key string like "s", "S", "ctrl+s", "alt+x" into a (`KeyCode`, `KeyModifiers`) pair.
pub fn parse_key(s: &str) -> Option<(KeyCode, KeyModifiers)> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("ctrl+") {
        let ch = rest.chars().next()?;
        return Some((
            KeyCode::Char(ch.to_ascii_lowercase()),
            KeyModifiers::CONTROL,
        ));
    }
    if let Some(rest) = s.strip_prefix("alt+") {
        let ch = rest.chars().next()?;
        return Some((KeyCode::Char(ch), KeyModifiers::ALT));
    }
    if s.len() == 1 {
        let ch = s.chars().next()?;
        let mods = if ch.is_uppercase() {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::empty()
        };
        return Some((KeyCode::Char(ch), mods));
    }
    None
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            duration_secs: DEFAULT_CACHE_SECS,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            tick_ms: DEFAULT_TICK_MS,
            repo_sort: crate::types::RepoSortKey::default(),
            checkout_dir: None,
            repo_columns: vec![crate::types::RepoColumn::LastPush],
            pr_columns: vec![
                crate::types::PrColumn::Comments,
                crate::types::PrColumn::CheckSummary,
                crate::types::PrColumn::DiffStats,
                crate::types::PrColumn::UpdatedAt,
                crate::types::PrColumn::Age,
            ],
            default_repo_view: crate::types::RepoView::default(),
            default_repos_view: crate::types::ReposView::default(),
            per_page: 0,
            merge_method: MergeMethod::default(),
            merge_auto: true,
            prefetch_pr_details: true,
            rate_limit_refresh_secs: DEFAULT_RATE_LIMIT_REFRESH_SECS,
        }
    }
}

impl Default for SourcesConfig {
    fn default() -> Self {
        Self {
            auto_fetch_orgs: true,
            include_self: true,
            orgs: vec![],
            users: vec![],
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct SourcesConfig {
    pub auto_fetch_orgs: bool,
    pub include_self: bool,
    pub orgs: Vec<String>,
    pub users: Vec<String>,
}

impl Config {
    pub const fn cache_ttl(&self) -> Duration {
        Duration::from_secs(self.cache.duration_secs)
    }

    pub const fn tick_interval(&self) -> Duration {
        Duration::from_millis(self.ui.tick_ms)
    }

    pub fn rate_limit_refresh_interval(&self) -> Duration {
        Duration::from_secs(
            self.ui
                .rate_limit_refresh_secs
                .max(MIN_RATE_LIMIT_REFRESH_SECS),
        )
    }
}

pub fn load() -> Config {
    let path = config_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Config::default();
    };
    match toml::from_str::<Config>(&text) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("ghview: config parse error in {}: {e}", path.display());
            Config::default()
        }
    }
}

pub fn config_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME").map_or_else(
        |_| {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home).join(".config")
        },
        PathBuf::from,
    );
    base.join("ghview").join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    #[test]
    fn parse_key_lowercase_no_modifiers() {
        let (code, mods) = parse_key("s").unwrap();
        assert_eq!(code, KeyCode::Char('s'));
        assert_eq!(mods, KeyModifiers::empty());
    }

    #[test]
    fn parse_key_uppercase_shift_modifier() {
        let (code, mods) = parse_key("S").unwrap();
        assert_eq!(code, KeyCode::Char('S'));
        assert_eq!(mods, KeyModifiers::SHIFT);
    }

    #[test]
    fn parse_key_ctrl_prefix() {
        let (code, mods) = parse_key("ctrl+s").unwrap();
        assert_eq!(code, KeyCode::Char('s'));
        assert_eq!(mods, KeyModifiers::CONTROL);
    }

    #[test]
    fn parse_key_alt_prefix() {
        let (code, mods) = parse_key("alt+x").unwrap();
        assert_eq!(code, KeyCode::Char('x'));
        assert_eq!(mods, KeyModifiers::ALT);
    }

    #[test]
    fn parse_key_trims_surrounding_whitespace() {
        assert!(parse_key("  s  ").is_some());
    }

    #[test]
    fn parse_key_invalid_returns_none() {
        assert!(parse_key("").is_none());
        assert!(parse_key("foo").is_none());
        assert!(parse_key("ctrl+").is_none());
    }

    fn pr_fixture() -> crate::types::PR {
        crate::types::PR {
            number: 42,
            title: "Fix bug".into(),
            author: "alice".into(),
            draft: false,
            state: crate::types::PrState::Open,
            created_at: "2024-01-01T00:00:00Z".into(),
            updated_at: "2024-01-01T00:00:00Z".into(),
            url: "https://github.com/myorg/myrepo/pull/42".into(),
            requested_reviewers: vec![],
            labels: vec![],
            head_ref: "fix/bug".into(),
            base_ref: "main".into(),
            head_sha: "abc123".into(),
            additions: 0,
            deletions: 0,
            comments: 0,
            repo: String::new(),
            repo_owner: String::new(),
        }
    }

    fn kb(cmd: &str) -> Keybinding {
        Keybinding {
            key: "o".into(),
            name: None,
            builtin: None,
            command: Some(cmd.into()),
            interactive: false,
        }
    }

    #[test]
    fn expand_command_pr_substitutes_all_placeholders() {
        let pr = pr_fixture();
        let expanded = kb("gh pr checkout {pr_number} --repo {owner}/{repo}")
            .expand_command(&PrContext {
                pr: &pr,
                owner: "myorg",
                repo: "myrepo",
            })
            .unwrap();
        assert_eq!(expanded, "gh pr checkout 42 --repo myorg/myrepo");
    }

    #[test]
    fn expand_command_pr_org_alias_for_owner() {
        let pr = pr_fixture();
        let expanded = kb("echo {org}")
            .expand_command(&PrContext {
                pr: &pr,
                owner: "myorg",
                repo: "myrepo",
            })
            .unwrap();
        assert_eq!(expanded, "echo myorg");
    }

    #[test]
    fn expand_command_repo_builds_url() {
        let expanded = kb("open {url}")
            .expand_command(&RepoContext {
                owner: "myorg",
                repo: "myrepo",
                language: Some("Rust"),
            })
            .unwrap();
        assert_eq!(expanded, "open https://github.com/myorg/myrepo");
    }

    #[test]
    fn expand_command_repo_language_placeholder() {
        let expanded = kb("echo {language}")
            .expand_command(&RepoContext {
                owner: "o",
                repo: "r",
                language: Some("Go"),
            })
            .unwrap();
        assert_eq!(expanded, "echo Go");
    }

    #[test]
    fn expand_command_repo_missing_language_empty_string() {
        let expanded = kb("echo {language}")
            .expand_command(&RepoContext {
                owner: "o",
                repo: "r",
                language: None,
            })
            .unwrap();
        assert_eq!(expanded, "echo ");
    }

    #[test]
    fn expand_command_none_when_no_command_set() {
        let no_cmd = Keybinding {
            key: "o".into(),
            name: None,
            builtin: Some("checkout".into()),
            command: None,
            interactive: false,
        };
        assert!(
            no_cmd
                .expand_command(&RepoContext {
                    owner: "o",
                    repo: "r",
                    language: None
                })
                .is_none()
        );
    }
}
