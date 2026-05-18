use serde::Deserialize;

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
    pub stars: u32,
    #[serde(default)]
    pub forks: u32,
    #[serde(default)]
    pub issues: u32,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default = "bool_true")]
    pub has_issues: bool,
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
    #[serde(rename = "login")]
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
    pub labels: Vec<String>,
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

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub state: String,
    pub created_at: String,
    pub labels: Vec<String>,
    pub url: String,
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
}

impl RepoSortKey {
    pub const fn next(self) -> Self {
        match self {
            Self::RecentlyUpdated => Self::Alphabetical,
            Self::Alphabetical => Self::RecentlyUpdated,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::RecentlyUpdated => "updated",
            Self::Alphabetical => "a-z",
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrColumn {
    DiffStats,
    Age,
    UpdatedAt,
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
    fn repo_sort_key_toggles() {
        assert_eq!(
            RepoSortKey::RecentlyUpdated.next(),
            RepoSortKey::Alphabetical
        );
        assert_eq!(
            RepoSortKey::Alphabetical.next(),
            RepoSortKey::RecentlyUpdated
        );
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
        owner: String,
        repo: String,
        prs: Vec<PR>,
        has_more: bool,
    },
    MorePrs {
        owner: String,
        repo: String,
        prs: Vec<PR>,
        page: u32,
        has_more: bool,
    },
    DiffContent {
        title: String,
        content: String,
    },
    ReviewStatus {
        owner: String,
        repo: String,
        pr_number: u64,
        status: ReviewStatus,
    },
    CheckRuns {
        pr_number: u64,
        runs: Vec<CheckRun>,
    },
    PrBody {
        pr_number: u64,
        body: String,
        mergeable_state: MergeableState,
        additions: u32,
        deletions: u32,
    },
    RepoFrontpage {
        owner: String,
        repo: String,
        description: String,
        readme: String,
    },
    Issues {
        owner: String,
        repo: String,
        issues: Vec<Issue>,
        has_more: bool,
    },
    MoreIssues {
        owner: String,
        repo: String,
        issues: Vec<Issue>,
        page: u32,
        has_more: bool,
    },
    IssueBody {
        number: u64,
        body: String,
    },
    RateLimit {
        remaining: u32,
        limit: u32,
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
