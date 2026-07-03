#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RAW="$ROOT/tests/fixtures/raw"
ORG="${GHVIEW_FIXTURE_ORG:?Set GHVIEW_FIXTURE_ORG to the org to capture from}"
ORG_REPO="${GHVIEW_FIXTURE_ORG_REPO:?Set GHVIEW_FIXTURE_ORG_REPO to owner/repo for the org repo}"
USER_LOGIN="${GHVIEW_FIXTURE_USER:?Set GHVIEW_FIXTURE_USER to the user to capture from}"
USER_REPO="${GHVIEW_FIXTURE_USER_REPO:?Set GHVIEW_FIXTURE_USER_REPO to owner/repo for the user repo}"

mkdir -p "$RAW"

echo "Capturing repos_org.jsonl..."
gh api "orgs/$ORG/repos?per_page=30&page=1&sort=pushed&direction=desc" --jq '.[] | {name, language, pushed_at, created_at, owner_login: .owner.login, stargazers_count, forks_count, open_issues_count, visibility, has_issues, has_pull_requests, archived, allow_auto_merge}' > "$RAW/repos_org.jsonl"

echo "Capturing repos_user.jsonl..."
gh api "users/$USER_LOGIN/repos?per_page=30&page=1&sort=pushed&direction=desc" --jq '.[] | {name, language, pushed_at, created_at, owner_login: .owner.login, stargazers_count, forks_count, open_issues_count, visibility, has_issues, has_pull_requests, archived, allow_auto_merge}' > "$RAW/repos_user.jsonl"

echo "Capturing prs.jsonl..."
gh api "repos/$ORG_REPO/pulls?state=open&per_page=30&page=1&sort=created&direction=desc" --jq '.[] | {number, title, author: (.user.login // "ghost"), draft, state, created_at, updated_at, url: .html_url, requested_reviewers: ([.requested_reviewers[] | .login] + [.requested_teams[] | .slug]), labels: [.labels[] | {name: .name, color: (.color // "8b949e")}], head_ref: .head.ref, base_ref: .base.ref, head_sha: .head.sha, comments: ((.comments // 0) + (.review_comments // 0))}' > "$RAW/prs.jsonl"

echo "Capturing source_prs.jsonl..."
gh api "search/issues?q=is:pr+is:open+author:$USER_LOGIN&sort=created&order=desc&per_page=30&page=1" --jq '.items[] | {number, title, author: (.user.login // "ghost"), state, created_at, updated_at, url: .html_url, labels: [.labels[] | {name: .name, color: (.color // "8b949e")}], comments: ((.comments // 0)), repo: (.repository_url | split("/") | .[-1]), repo_owner: (.repository_url | split("/") | .[-2])}' > "$RAW/source_prs.jsonl"

echo "Capturing issues.jsonl..."
gh api "repos/$ORG_REPO/issues?state=open&per_page=30&page=1" --jq '.[] | {number, title, author: (.user.login // "ghost"), created_at, labels: [.labels[] | {name: .name, color: (.color // "8b949e")}], url: .html_url, is_pr: (.pull_request != null)}' > "$RAW/issues.jsonl"

echo "Capturing source_issues.jsonl..."
gh api "search/issues?q=is:issue+is:open+org:$ORG&sort=created&order=desc&per_page=30&page=1" --jq '.items[] | {number, title, author: (.user.login // "ghost"), created_at, labels: [.labels[] | {name: .name, color: (.color // "8b949e")}], url: .html_url, repo: (.repository_url | split("/") | .[-1]), repo_owner: (.repository_url | split("/") | .[-2])}' > "$RAW/source_issues.jsonl"

echo "Fetching PR number..."
PR_NUMBER="$(gh api "repos/$ORG_REPO/pulls?state=open&per_page=1&page=1" --jq '.[0].number')"
if [ -z "$PR_NUMBER" ] || [ "$PR_NUMBER" = "null" ]; then
  echo "ERROR: No open PRs found, cannot continue"
  exit 1
fi

echo "Capturing pr_body.json..."
gh api "repos/$ORG_REPO/pulls/$PR_NUMBER" --jq '{body: (.body // ""), mergeable_state: (.mergeable_state // "unknown"), additions: (.additions // 0), deletions: (.deletions // 0), head_sha: .head.sha}' > "$RAW/pr_body.json"

echo "Capturing review_states.txt..."
gh api "repos/$ORG_REPO/pulls/$PR_NUMBER/reviews?per_page=100" --jq '.[] | .state' > "$RAW/review_states.txt"

echo "Fetching HEAD SHA for PR $PR_NUMBER..."
HEAD_SHA="$(gh api "repos/$ORG_REPO/pulls/$PR_NUMBER" --jq '.head.sha')"

echo "Capturing check_runs.json..."
gh api "repos/$ORG_REPO/commits/$HEAD_SHA/check-runs" --jq '[.check_runs[] | {id: .id, name: .name, url: .html_url, suite_id: .check_suite.id, s: (if .conclusion == "failure" or .conclusion == "cancelled" or .conclusion == "timed_out" or .conclusion == "action_required" then "failing" elif .status == "in_progress" or .status == "queued" then "pending" elif .conclusion == "success" or .conclusion == "neutral" or .conclusion == "skipped" then "passing" else "unknown" end)}]' > "$RAW/check_runs.json"

echo "Capturing workflow_runs.json..."
gh api "repos/$ORG_REPO/actions/runs?head_sha=$HEAD_SHA" --jq '[.workflow_runs[] | {name, event, suite_id: .check_suite_id}]' > "$RAW/workflow_runs.json"

echo "Capturing diff.txt..."
GH_PAGER="" NO_COLOR=1 gh pr diff "$PR_NUMBER" -R "$ORG_REPO" > "$RAW/diff.txt"

echo "Capturing readme.md..."
gh api "repos/$ORG_REPO/readme" --jq '.content | gsub("\n";"") | @base64d' > "$RAW/readme.md"

echo "Capturing repo_description.txt..."
gh api "repos/$ORG_REPO" --jq '.description // ""' > "$RAW/repo_description.txt"

echo "Fetching issue number from user repo..."
ISSUE_NUMBER="$(gh api "repos/$USER_REPO/issues?state=all&per_page=1&page=1" --jq '.[0].number')"
if [ -z "$ISSUE_NUMBER" ] || [ "$ISSUE_NUMBER" = "null" ]; then
  echo "ERROR: No issues found in $USER_REPO, cannot continue"
  exit 1
fi

echo "Capturing issue_body.md..."
gh api "repos/$USER_REPO/issues/$ISSUE_NUMBER" --jq '.body // ""' > "$RAW/issue_body.md"

echo "Fixture capture complete. Raw directory contents:"
ls -la "$RAW"
