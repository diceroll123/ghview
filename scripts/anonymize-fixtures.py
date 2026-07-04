#!/usr/bin/env python3
"""Anonymize fixture data by replacing sensitive values with deterministic placeholders."""

from __future__ import annotations

import json
import re
import hashlib
import os
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path


JsonValue = str | int | float | bool | None | list["JsonValue"] | dict[str, "JsonValue"]


def main() -> None:
    repo_root = Path(__file__).resolve().parent.parent
    raw_dir = repo_root / "tests" / "fixtures" / "raw"
    out_dir = repo_root / "tests" / "fixtures"

    raw_files = sorted(f.name for f in raw_dir.iterdir() if f.is_file() and f.name != ".gitkeep")
    if not raw_files:
        print("Raw fixtures directory is empty. Nothing to do.")
        sys.exit(0)

    # Constants
    FIXED_NOW = datetime(2026, 1, 15, 12, 0, 0, tzinfo=timezone.utc)
    TIME_OFFSETS = [
        timedelta(minutes=45),
        timedelta(hours=3),
        timedelta(hours=26),
        timedelta(days=3),
        timedelta(days=14),
        timedelta(days=180),
        timedelta(days=730),
    ]
    ORG_LOGINS = {s.strip() for s in os.environ.get("GHVIEW_FIXTURE_ORGS", "").split(",") if s.strip()}
    CAPTURE_USER = os.environ.get("GHVIEW_FIXTURE_USER", "")
    PRESERVED_USERS = {"dependabot[bot]", "ghost"}

    NATO_ALPHABET = [
        "alpha", "bravo", "charlie", "delta", "echo", "foxtrot",
        "golf", "hotel", "india", "juliett", "kilo", "lima",
        "mike", "november", "oscar", "papa", "quebec", "romeo",
        "sierra", "tango", "uniform", "victor", "whiskey", "xray",
        "yankee", "zulu"
    ]

    FAKE_TITLES = [
        "Fix panic when list is empty",
        "Add keyboard navigation to sidebar",
        "chore(deps): bump serde from 1.0.1 to 1.0.2",
        "feat: add dark mode support",
        "refactor: simplify auth flow",
        "docs: update README with installation steps",
        "fix: resolve memory leak in cache",
        "test: add integration tests for API",
        "style: format code with rustfmt",
        "perf: optimize database queries",
        "chore: remove unused dependencies",
        "feat: implement search functionality",
        "bugfix: handle null pointer exception",
        "refactor: extract service layer",
        "docs: add contributing guidelines",
        "test: fix flaky unit tests",
        "feat: add logging middleware",
        "fix: correct timezone handling",
        "chore: update CI configuration",
        "refactor: clean up legacy code",
    ]

    # Known file order
    KNOWN_FILES = [
        "repos_org.jsonl", "repos_user.jsonl", "prs.jsonl",
        "source_prs.jsonl", "issues.jsonl", "source_issues.jsonl",
        "pr_body.json", "review_states.txt", "check_runs.json",
        "workflow_runs.json", "diff.txt", "readme.md",
        "repo_description.txt", "issue_body.md"
    ]

    # Collect all files in raw dir
    all_files = raw_files
    ordered_files = [f for f in KNOWN_FILES if f in all_files]
    extra_files = [f for f in all_files if f not in KNOWN_FILES]
    files_to_process = ordered_files + sorted(extra_files)

    # === PASS 1: Collect data from JSON/JSONL files ===
    usernames: list[str] = []
    repos: list[str] = []
    timestamps: list[str] = []
    shas: list[str] = []
    ids: list[int] = []
    titles: list[str] = []

    def add_unique(lst: list, val: object) -> None:
        if val is not None and val != "" and val not in lst:
            lst.append(val)

    for filename in files_to_process:
        filepath = raw_dir / filename
        ext = filepath.suffix.lower()

        if ext in (".json", ".jsonl"):
            try:
                content = filepath.read_text()
                lines = content.split("\n")

                for line in lines:
                    if not line.strip():
                        continue

                    try:
                        obj = json.loads(line)
                    except json.JSONDecodeError:
                        continue

                    # Walk the object recursively
                    def process_node(node: JsonValue) -> None:
                        if isinstance(node, dict):
                            for key, val in node.items():
                                process_key_value(key, val)
                                process_node(val)
                        elif isinstance(node, list):
                            for item in node:
                                process_node(item)

                    def process_key_value(key: str, val: JsonValue) -> None:
                        # Username fields
                        if key in ("author", "owner_login", "repo_owner"):
                            if isinstance(val, str) and val not in PRESERVED_USERS:
                                add_unique(usernames, val)
                        elif key == "requested_reviewers":
                            if isinstance(val, list):
                                for v in val:
                                    if isinstance(v, str) and v not in PRESERVED_USERS:
                                        add_unique(usernames, v)

                        # Repo name fields (only repos_*.jsonl)
                        if filename.startswith("repos_") and key == "name":
                            if isinstance(val, str):
                                add_unique(repos, val)
                        elif key == "repo":
                            if isinstance(val, str):
                                add_unique(repos, val)

                        # Timestamp fields ending with _at
                        if key.endswith("_at"):
                            if isinstance(val, str) and re.match(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z?$", val):
                                add_unique(timestamps, val)

                        # SHA fields - collect all 40-char hex SHAs from any string value
                        if isinstance(val, str):
                            for match in re.findall(r"\b[0-9a-f]{40}\b", val):
                                add_unique(shas, match)

                        # ID fields in check_runs and workflow_runs
                        if filename in ("check_runs.json", "workflow_runs.json"):
                            if key == "id" or key == "suite_id":
                                if isinstance(val, int) and val not in ids:
                                    ids.append(val)

                        # Title fields
                        if key == "title":
                            if isinstance(val, str):
                                add_unique(titles, val)

                    process_node(obj)
            except Exception as e:
                pass  # Skip files that can't be parsed

    # Scan raw text files (non-json/jsonl) for additional 40-char SHAs
    for filename in files_to_process:
        filepath = raw_dir / filename
        ext = filepath.suffix.lower()
        if ext not in (".json", ".jsonl"):
            try:
                content = filepath.read_text()
                for match in re.findall(r"\b[0-9a-f]{40}\b", content):
                    add_unique(shas, match)
            except Exception:
                pass

    # Also extract usernames from URLs in JSONL files
    url_pattern = r"(?<!api\.)github\.com/([^/]+)/([^/?]+)"
    api_url_pattern = r"api\.github\.com/repos/([^/]+)/([^/?#]+)"
    for filename in files_to_process:
        filepath = raw_dir / filename
        ext = filepath.suffix.lower()

        if ext in (".json", ".jsonl"):
            try:
                content = filepath.read_text()
                matches = re.findall(url_pattern, content, re.IGNORECASE)
                for owner, repo in matches:
                    if owner in ("repos", "orgs", "users"):
                        continue
                    add_unique(usernames, owner)
                    add_unique(repos, repo)
                api_matches = re.findall(api_url_pattern, content, re.IGNORECASE)
                for owner, repo in api_matches:
                    add_unique(usernames, owner)
                    add_unique(repos, repo)
            except Exception:
                pass

    # Build mappings
    username_map: dict[str, str] = {}
    user_counter = 0
    for u in usernames:
        if u in ORG_LOGINS:
            username_map[u] = "octo-org"
        elif CAPTURE_USER and u == CAPTURE_USER:
            username_map[u] = "octocat"
        elif u in PRESERVED_USERS:
            username_map[u] = u
        else:
            username_map[u] = f"user-{user_counter + 1:02d}"
            user_counter += 1

    repo_map: dict[str, str] = {}
    repo_counter = 0
    for r in repos:
        if r not in repo_map:
            base_name = NATO_ALPHABET[repo_counter % len(NATO_ALPHABET)]
            if repo_counter >= len(NATO_ALPHABET):
                repo_map[r] = f"repo-{base_name}-{(repo_counter // len(NATO_ALPHABET)) + 1}"
            else:
                repo_map[r] = f"repo-{base_name}"
            repo_counter += 1

    timestamp_map: dict[str, str] = {}
    for i, ts in enumerate(timestamps):
        dt = datetime.fromisoformat(ts.replace("Z", "+00:00"))
        offset = TIME_OFFSETS[i % len(TIME_OFFSETS)]
        new_dt = FIXED_NOW - offset
        timestamp_map[ts] = new_dt.strftime("%Y-%m-%dT%H:%M:%SZ")

    sha_map: dict[str, str] = {}
    for i, sha in enumerate(shas):
        sha_map[sha] = hashlib.sha1(str(i).encode()).hexdigest()

    # Helper functions for SHA anonymization
    HEX_RE = re.compile(r"\b[0-9a-f]{7,40}\b")

    def fake_sha_for(tok: str) -> str:
        # full sha known
        if tok in sha_map:
            return sha_map[tok]
        # short token that is a prefix of a known real sha -> same-length prefix of its fake
        for real, fake in sha_map.items():
            if real.startswith(tok):
                return fake[:len(tok)]
        # unknown hex token: deterministic hash, truncated to same length
        h = hashlib.sha1(tok.encode()).hexdigest()
        return (h * ((len(tok) // 40) + 1))[:len(tok)]

    def anonymize_shas(text: str) -> str:
        def repl(m) -> str:
            tok = m.group(0)
            if not re.search(r"\d", tok):
                return tok  # avoid mangling english words like 'acceded'
            return fake_sha_for(tok)
        return HEX_RE.sub(repl, text)

    id_map: dict[int, int] = {}
    for i, uid in enumerate(ids):
        id_map[uid] = 100001 + i

    title_map: dict[str, str] = {}
    for i, t in enumerate(titles):
        title_map[t] = FAKE_TITLES[i % len(FAKE_TITLES)]

    # === PASS 2: Rewrite files ===

    def transform_value(key: str, val: JsonValue, branch: bool = False) -> JsonValue:
        """Transform a value based on its key."""
        if key in ("id", "suite_id") and isinstance(val, int):
            return id_map.get(val, val)
        if isinstance(val, str):
            new_val = val

            # url/html_url/repository_url fields are rewritten positionally
            # later by rewrite_url() (owner and repo looked up independently).
            # Skipping the blind substring passes here avoids corruption when
            # the org login and repo name are identical (e.g. ratatui/ratatui),
            # which would otherwise collapse both path segments to the same
            # replacement before rewrite_url ever sees the original text.
            if key not in ("url", "html_url", "repository_url"):
                # Apply username map
                for old_u, new_u in username_map.items():
                    new_val = re.sub(rf"\b{re.escape(old_u)}\b", new_u, new_val)

                # Apply repo map
                for old_r, new_r in repo_map.items():
                    new_val = re.sub(rf"\b{re.escape(old_r)}\b", new_r, new_val)

            # Apply timestamp map (for *_at fields)
            if key.endswith("_at") and val in timestamp_map:
                return timestamp_map[val]

            # Apply ID map
            if key in ("id", "suite_id"):
                return id_map.get(val, val)

            # Apply title map
            if key == "title" and val in title_map:
                return title_map[val]

            # Branch name transformation
            if branch:
                for old_u, new_u in username_map.items():
                    new_val = re.sub(rf"\b{re.escape(old_u)}\b", new_u, new_val)
                for old_r, new_r in repo_map.items():
                    new_val = re.sub(rf"\b{re.escape(old_r)}\b", new_r, new_val)

            # SHA anonymization
            new_val = anonymize_shas(new_val)

            return new_val

        elif isinstance(val, list):
            new_list = []
            for item in val:
                new_item = transform_value(key, item, branch=(key == "requested_reviewers"))
                new_list.append(new_item)
            return new_list
        elif isinstance(val, dict):
            new_dict = {}
            for k, v in val.items():
                if key in ("username", "repo_owner"):
                    new_v = transform_value(k, v, branch=True)
                else:
                    new_v = transform_value(k, v, branch=(k in ("head_ref", "base_ref")))
                new_dict[k] = new_v
            return new_dict

        return val

    def rewrite_json_object(obj: JsonValue) -> JsonValue:
        """Recursively rewrite a JSON object."""
        if isinstance(obj, dict):
            new_obj = {}
            for k, v in obj.items():
                # branch names need special handling for URL-like content
                if k in ("head_ref", "base_ref"):
                    new_v = transform_value(k, v, branch=True)
                elif k in ("url", "html_url", "repository_url"):
                    # Leave untouched here; transform_urls() rewrites these
                    # from the original text so it can tell owner from repo.
                    # Running the generic word-substitution pass first would
                    # corrupt this when a repo shares its org's name (e.g.
                    # "ratatui/ratatui"): the org->placeholder substitution
                    # would already replace the repo segment too.
                    new_v = v
                else:
                    new_v = transform_value(k, v)
                new_obj[k] = new_v
            return new_obj
        elif isinstance(obj, list):
            return [rewrite_json_object(item) for item in obj]
        return obj

    def rewrite_url(url) -> JsonValue:
        """Rewrite URLs with mapped usernames and repos."""
        if not isinstance(url, str):
            return url

        # Match github.com/owner/repo/... patterns
        pattern = r"(https?://)?(www\.)?(github\.com/)([^/]+)/([^/?#]+)(.*)"
        match = re.match(pattern, url, re.IGNORECASE)
        if match:
            prefix, www, domain, owner, repo, rest = (g or "" for g in match.groups())
            new_owner = username_map.get(owner, owner)
            # Map repos in URL path too
            new_repo = repo_map.get(repo, repo)
            return f"{prefix}{www}{domain}{new_owner}/{new_repo}{rest}"

        # Match api.github.com/repos/owner/repo patterns
        pattern2 = r"(https?://)?(api\.github\.com/repos/)([^/]+)/([^/?#]+)(.*)"
        match2 = re.match(pattern2, url, re.IGNORECASE)
        if match2:
            prefix, domain, owner, repo, rest = (g or "" for g in match2.groups())
            new_owner = username_map.get(owner, owner)
            new_repo = repo_map.get(repo, repo)
            return f"{prefix}{domain}{new_owner}/{new_repo}{rest}"

        # Match repository_url style: https://api.github.com/repos/{owner}/{repo}
        pattern3 = r"https?://api\.github\.com/repos/([^/]+)/([^/?#]+)"
        match3 = re.search(pattern3, url)
        if match3:
            owner, repo = match3.groups()
            new_owner = username_map.get(owner, owner)
            new_repo = repo_map.get(repo, repo)
            return url.replace(f"repos/{owner}/{repo}", f"repos/{new_owner}/{new_repo}")

        return url

    def process_file(filename: str) -> None:
        filepath = raw_dir / filename
        out_path = out_dir / filename
        ext = filepath.suffix.lower()

        if ext in (".json", ".jsonl"):
            content = filepath.read_text()
            lines = content.split("\n")

            if ext == ".jsonl":
                output_lines = []
                for line in lines:
                    if not line.strip():
                        continue
                    try:
                        obj = json.loads(line)
                        new_obj = rewrite_json_object(obj)
                        # Rewrite URLs in the object
                        def transform_urls(node: JsonValue) -> None:
                            if isinstance(node, dict):
                                for k, v in node.items():
                                    if k in ("url", "html_url", "repository_url") and isinstance(v, str):
                                        node[k] = rewrite_url(v)
                                    else:
                                        transform_urls(v)
                            elif isinstance(node, list):
                                for item in node:
                                    transform_urls(item)
                        transform_urls(new_obj)
                        output_lines.append(json.dumps(new_obj, separators=(",", ":")))
                    except json.JSONDecodeError:
                        # Keep malformed lines as-is
                        output_lines.append(line)

                out_path.write_text("\n".join(output_lines) + "\n" if output_lines else "")
            else:  # .json
                try:
                    obj = json.loads(content)
                    new_obj = rewrite_json_object(obj)

                    # Rewrite URLs in the object
                    def transform_urls(node: JsonValue) -> None:
                        if isinstance(node, dict):
                            for k, v in node.items():
                                if k in ("url", "html_url", "repository_url") and isinstance(v, str):
                                    node[k] = rewrite_url(v)
                                else:
                                    transform_urls(v)
                        elif isinstance(node, list):
                            for item in node:
                                transform_urls(item)
                    transform_urls(new_obj)

                    out_path.write_text(json.dumps(new_obj, separators=(",", ":")) + "\n")
                except json.JSONDecodeError:
                    # Keep malformed JSON as-is
                    out_path.write_text(content)
        else:
            # Text files: apply substitution
            content = filepath.read_text()
            original_content = content

            # Collect all patterns to substitute (longest first)
            patterns_to_replace = list(username_map.items()) + list(repo_map.items())

            # Sort by length descending to avoid partial overlap issues
            patterns_to_replace.sort(key=lambda x: -len(x[0]))

            for old, new in patterns_to_replace:
                content = re.sub(re.escape(old), new, content, flags=re.IGNORECASE)

            # Anonymize all SHAs (7-40 char hex) in text files
            content = anonymize_shas(content)

            out_path.write_text(content)

    for filename in files_to_process:
        process_file(filename)

    # === FINAL RESIDUAL SWEEP ===
    # Catch prose mentions (readme, issue bodies, diffs) that the structured
    # passes miss. Longest names first so substring names cannot clobber
    # longer ones (e.g. a repo named after its owner plus a suffix).
    residual_terms: list[tuple[str, str]] = []
    for old, new in list(repo_map.items()) + list(username_map.items()):
        if old != new:
            residual_terms.append((old, new))
    # Extra prose terms that cannot be derived from the data, e.g. a product
    # name mentioned in READMEs. Format: "old1=new1,old2=new2".
    for pair in os.environ.get("GHVIEW_FIXTURE_EXTRA_TERMS", "").split(","):
        if "=" in pair:
            old, new = pair.split("=", 1)
            if old.strip():
                residual_terms.append((old.strip(), new.strip()))
    residual_terms.sort(key=lambda x: -len(x[0]))
    residual = [(re.compile(re.escape(old), re.IGNORECASE), new) for old, new in residual_terms]

    for filename in files_to_process:
        filepath = out_dir / filename
        if not filepath.exists():
            continue
        content = filepath.read_text()
        for pat, new in residual:
            content = pat.sub(new, content)
        filepath.write_text(content)

    # === ACCEPTANCE TEST ===
    leak_names: set[str] = {old for old, new in residual_terms} | ORG_LOGINS
    if CAPTURE_USER:
        leak_names.add(CAPTURE_USER)
    leak_names = {n for n in leak_names if n}
    if leak_names:
        pattern = re.compile("|".join(re.escape(n) for n in sorted(leak_names, key=len, reverse=True)), re.IGNORECASE)
    else:
        pattern = re.compile(r"(?!x)x")  # matches nothing
    leak_found = False

    for filename in files_to_process:
        filepath = out_dir / filename
        if not filepath.exists():
            continue

        content = filepath.read_text()
        for line in content.split("\n"):
            if pattern.search(line):
                # Truncate long lines for display
                snippet = line[:80] + "..." if len(line) > 80 else line
                print(f"LEAK: {filename}: {snippet}")
                leak_found = True

        # Verify no real SHA leaked
        for real in shas:
            for n in (7, 8, 9, 10, 11, 12, 40):
                if real[:n] in content:
                    print(f"LEAK: {filename}: sha {real[:n]}")
                    leak_found = True
                    break

    if leak_found:
        sys.exit(1)

    print("OK: fixtures clean")


if __name__ == "__main__":
    main()
