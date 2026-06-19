use serde::Deserialize;
use std::fmt;

fn strip_variation_selectors<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    Ok(s.chars()
        .filter(|&c| !('\u{FE00}'..='\u{FE0F}').contains(&c))
        .collect())
}

#[derive(Debug, Clone, Deserialize)]
pub struct Label {
    #[serde(deserialize_with = "strip_variation_selectors")]
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    #[default]
    Public,
    Private,
    Internal,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Repo {
    pub name: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub pushed_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub stars: u32,
    #[serde(default)]
    pub forks: u32,
    #[serde(default)]
    pub issues: u32,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default = "bool_true")]
    pub has_issues: bool,
    #[serde(default = "bool_true")]
    pub has_pull_requests: bool,
    #[serde(default)]
    pub archived: bool,
}

fn bool_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PrState {
    #[default]
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MergeableState {
    Clean,
    Behind,
    Dirty,
    Blocked,
    Unstable,
    HasHooks,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PR {
    pub number: u64,
    pub title: String,
    pub author: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub state: PrState,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    pub url: String,
    #[serde(default)]
    pub requested_reviewers: Vec<String>,
    #[serde(default)]
    pub labels: Vec<Label>,
    #[serde(default)]
    pub head_ref: String,
    #[serde(default)]
    pub base_ref: String,
    #[serde(default)]
    pub head_sha: String,
    #[serde(default)]
    pub additions: u32,
    #[serde(default)]
    pub deletions: u32,
    #[serde(default)]
    pub comments: u32,
    /// Populated for source-level PR lists; empty for per-repo lists.
    #[serde(default)]
    pub repo: String,
    /// Actual repo owner; populated for source-level PR lists, empty for per-repo lists.
    #[serde(default)]
    pub repo_owner: String,
}

impl PR {
    pub fn is_dimmed(&self) -> bool {
        self.draft || self.state == PrState::Closed
    }
}

/// Identifies a GitHub repo by owner and repo name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepoId {
    pub owner: String,
    pub repo: String,
}

impl RepoId {
    pub fn new(owner: impl Into<String>, repo: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            repo: repo.into(),
        }
    }

    pub fn pr(self, number: u64) -> PrId {
        PrId { repo: self, number }
    }

    pub fn key(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    pub fn url(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.repo)
    }

    pub fn issues_url(&self) -> String {
        format!("https://github.com/{}/{}/issues", self.owner, self.repo)
    }

    pub fn api_base(&self) -> String {
        format!("repos/{}/{}", self.owner, self.repo)
    }
}

impl fmt::Display for RepoId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.repo)
    }
}

impl From<(String, String)> for RepoId {
    fn from((owner, repo): (String, String)) -> Self {
        Self { owner, repo }
    }
}

/// Identifies a specific PR within a repo.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrId {
    pub repo: RepoId,
    pub number: u64,
}

impl fmt::Display for PrId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.repo, self.number)
    }
}

impl From<(RepoId, u64)> for PrId {
    fn from((repo, number): (RepoId, u64)) -> Self {
        Self { repo, number }
    }
}

/// A source in the leftmost column — either the current user or an org.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    User(String),
    Org(String),
}

impl Source {
    /// The GitHub owner name (used in API paths like repos/{owner}/{repo}).
    pub fn owner(&self) -> &str {
        match self {
            Self::User(n) | Self::Org(n) => n,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::User(n) => format!("@{n}"),
            Self::Org(n) => n.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Sources,
    Repos,
    Repo,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RepoView {
    Frontpage,
    #[default]
    Prs,
    Issues,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Default)]
pub enum ReposView {
    #[default]
    #[serde(rename = "repos")]
    RepoList,
    #[serde(rename = "prs")]
    PrList,
    #[serde(rename = "issues")]
    IssueList,
}

impl ReposView {
    pub fn switch_action(self) -> crate::keys::Action {
        match self {
            Self::RepoList => crate::keys::Action::ViewRepos,
            Self::PrList => crate::keys::Action::ViewPrs,
            Self::IssueList => crate::keys::Action::ViewIssues,
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub state: String,
    pub created_at: String,
    pub labels: Vec<Label>,
    pub url: String,
    /// Repo name; populated for source-level issue lists, empty for per-repo lists.
    #[serde(default)]
    pub repo: String,
    /// Actual repo owner; populated for source-level issue lists, empty for per-repo lists.
    #[serde(default)]
    pub repo_owner: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailSection {
    #[default]
    Body,
    Checks,
}

#[derive(Debug, Clone)]
pub struct CheckRun {
    pub id: u64,
    pub name: String,
    pub url: String,
    pub status: CheckStatus,
}

#[derive(Debug, Clone)]
pub enum LoadingKind {
    Sources,
    Repos,
    Frontpage,
    Prs,
    Issues,
    Action(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewStatus {
    Approved,
    ChangesRequested,
    Pending,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Passing,
    Failing,
    Pending,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Newest,
    RecentlyUpdated,
    Oldest,
    LeastReviewed,
}

impl SortKey {
    pub const fn next(self) -> Self {
        match self {
            Self::Newest => Self::RecentlyUpdated,
            Self::RecentlyUpdated => Self::Oldest,
            Self::Oldest => Self::LeastReviewed,
            Self::LeastReviewed => Self::Newest,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Newest => "newest",
            Self::RecentlyUpdated => "updated",
            Self::Oldest => "oldest",
            Self::LeastReviewed => "needs review",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RepoSortKey {
    #[default]
    #[serde(alias = "updated")]
    RecentlyUpdated,
    #[serde(alias = "alpha")]
    Alphabetical,
    Created,
}

impl RepoSortKey {
    pub const fn next(self) -> Self {
        match self {
            Self::RecentlyUpdated => Self::Alphabetical,
            Self::Alphabetical => Self::Created,
            Self::Created => Self::RecentlyUpdated,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::RecentlyUpdated => "pushed",
            Self::Alphabetical => "a-z",
            Self::Created => "created",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoColumn {
    Stars,
    Forks,
    Issues,
    Visibility,
    LastPush,
    Created,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrColumn {
    DiffStats,
    Age,
    UpdatedAt,
    Comments,
    CheckSummary,
}

#[derive(Debug, Clone, Copy)]
pub enum PrAction {
    Approve,
    Merge,
    Close,
    Reopen,
    MarkReady,
}

impl PrAction {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Merge => "merge",
            Self::Close => "close",
            Self::Reopen => "reopen",
            Self::MarkReady => "ready",
        }
    }

    pub fn success_msg(self, pr_number: u64) -> String {
        match self {
            Self::Approve => format!("✓ Approved #{pr_number}"),
            Self::Merge => format!("✓ Merged #{pr_number}"),
            Self::Close => format!("✓ Closed #{pr_number}"),
            Self::Reopen => format!("✓ Reopened #{pr_number}"),
            Self::MarkReady => format!("✓ Marked ready #{pr_number}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_owner_returns_inner_name() {
        assert_eq!(Source::User("alice".into()).owner(), "alice");
        assert_eq!(Source::Org("my-org".into()).owner(), "my-org");
    }

    #[test]
    fn source_display_user_prefixed_with_at() {
        assert_eq!(Source::User("alice".into()).display(), "@alice");
    }

    #[test]
    fn source_display_org_plain() {
        assert_eq!(Source::Org("my-org".into()).display(), "my-org");
    }

    #[test]
    fn sort_key_cycles_all_variants() {
        let k = SortKey::Newest;
        let k = k.next();
        assert_eq!(k, SortKey::RecentlyUpdated);
        let k = k.next();
        assert_eq!(k, SortKey::Oldest);
        let k = k.next();
        assert_eq!(k, SortKey::LeastReviewed);
        let k = k.next();
        assert_eq!(k, SortKey::Newest);
    }

    #[test]
    fn repo_sort_key_cycles_all_variants() {
        assert_eq!(
            RepoSortKey::RecentlyUpdated.next(),
            RepoSortKey::Alphabetical
        );
        assert_eq!(RepoSortKey::Alphabetical.next(), RepoSortKey::Created);
        assert_eq!(RepoSortKey::Created.next(), RepoSortKey::RecentlyUpdated);
    }

    #[test]
    fn pr_action_labels_all_variants() {
        assert_eq!(PrAction::Approve.label(), "approve");
        assert_eq!(PrAction::Merge.label(), "merge");
        assert_eq!(PrAction::Close.label(), "close");
        assert_eq!(PrAction::Reopen.label(), "reopen");
        assert_eq!(PrAction::MarkReady.label(), "ready");
    }

    #[test]
    fn pr_action_success_msg_contains_pr_number() {
        assert!(PrAction::Merge.success_msg(42).contains("42"));
        assert!(PrAction::Approve.success_msg(99).contains("99"));
    }

    // RepoId
    #[test]
    fn repoid_display_slash_separated() {
        assert_eq!(RepoId::new("owner", "repo").to_string(), "owner/repo");
    }

    #[test]
    fn repoid_key_matches_display() {
        let rid = RepoId::new("alice", "myrepo");
        assert_eq!(rid.key(), rid.to_string());
    }

    #[test]
    fn repoid_from_tuple() {
        let rid = RepoId::from(("alice".to_string(), "myrepo".to_string()));
        assert_eq!(rid.owner, "alice");
        assert_eq!(rid.repo, "myrepo");
    }

    #[test]
    fn repoid_eq_same_values() {
        assert_eq!(RepoId::new("a", "b"), RepoId::new("a", "b"));
    }

    #[test]
    fn repoid_ne_different_owner() {
        assert_ne!(RepoId::new("a", "b"), RepoId::new("x", "b"));
    }

    #[test]
    fn repoid_ne_different_repo() {
        assert_ne!(RepoId::new("a", "b"), RepoId::new("a", "x"));
    }

    #[test]
    fn repoid_hash_consistent_with_eq() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(RepoId::new("a", "b"));
        assert!(set.contains(&RepoId::new("a", "b")));
        assert!(!set.contains(&RepoId::new("a", "c")));
    }

    // PrId
    #[test]
    fn prid_display_includes_repo_and_number() {
        let pr = RepoId::new("owner", "repo").pr(42);
        assert_eq!(pr.to_string(), "owner/repo#42");
    }

    #[test]
    fn prid_from_tuple() {
        let pr = PrId::from((RepoId::new("a", "b"), 7u64));
        assert_eq!(pr.number, 7);
        assert_eq!(pr.repo, RepoId::new("a", "b"));
    }

    #[test]
    fn prid_constructor_chain() {
        let pr = RepoId::new("org", "lib").pr(99);
        assert_eq!(pr.repo.owner, "org");
        assert_eq!(pr.repo.repo, "lib");
        assert_eq!(pr.number, 99);
    }

    #[test]
    fn prid_eq_same_values() {
        assert_eq!(RepoId::new("a", "b").pr(1), RepoId::new("a", "b").pr(1),);
    }

    #[test]
    fn prid_ne_different_number() {
        assert_ne!(RepoId::new("a", "b").pr(1), RepoId::new("a", "b").pr(2));
    }

    #[test]
    fn prid_ne_different_repo() {
        assert_ne!(RepoId::new("a", "b").pr(1), RepoId::new("a", "c").pr(1));
    }

    #[test]
    fn prid_hash_usable_as_map_key() {
        use std::collections::HashMap;
        let mut map: HashMap<PrId, &str> = HashMap::new();
        let k = RepoId::new("a", "b").pr(1);
        map.insert(k.clone(), "val");
        assert_eq!(map.get(&k), Some(&"val"));
        assert_eq!(map.get(&RepoId::new("a", "b").pr(2)), None);
    }

    #[test]
    fn repoid_new_accepts_string_and_str() {
        let owned = "owner".to_string();
        let rid = RepoId::new(owned.clone(), "repo");
        assert_eq!(rid.owner, owned);
    }
}

#[derive(Debug)]
pub enum DataMsg {
    Sources {
        sources: Vec<Source>,
        current_user: String,
    },
    Repos {
        owner: String,
        repos: Vec<Repo>,
        has_more: bool,
    },
    MoreRepos {
        owner: String,
        repos: Vec<Repo>,
        has_more: bool,
    },
    Prs {
        repo: RepoId,
        prs: Vec<PR>,
        has_more: bool,
    },
    MorePrs {
        repo: RepoId,
        prs: Vec<PR>,
        has_more: bool,
    },
    DiffContent {
        pr: PrId,
        title: String,
        content: String,
    },
    ReviewStatus {
        pr: PrId,
        status: ReviewStatus,
    },
    CheckRuns {
        pr: PrId,
        runs: Vec<CheckRun>,
    },
    PrBody {
        pr: PrId,
        body: String,
        mergeable_state: MergeableState,
        additions: u32,
        deletions: u32,
    },
    RepoFrontpage {
        repo: RepoId,
        description: String,
        readme: String,
    },
    Issues {
        repo: RepoId,
        issues: Vec<Issue>,
        has_more: bool,
    },
    MoreIssues {
        repo: RepoId,
        issues: Vec<Issue>,
        has_more: bool,
    },
    IssueBody {
        repo: RepoId,
        number: u64,
        body: String,
    },
    RateLimit {
        remaining: u32,
        limit: u32,
    },
    ViewerPermission {
        repo: RepoId,
        can_push: bool,
    },
    SourcePrs {
        owner: String,
        prs: Vec<PR>,
        has_more: bool,
    },
    MoreSourcePrs {
        owner: String,
        prs: Vec<PR>,
        has_more: bool,
    },
    SourceIssues {
        owner: String,
        issues: Vec<Issue>,
        has_more: bool,
    },
    MoreSourceIssues {
        owner: String,
        issues: Vec<Issue>,
        has_more: bool,
    },
    ActionDone(Option<String>),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct DiffView {
    pub title: String,
    pub lines: Box<[String]>,
    pub scroll: u16,
}
