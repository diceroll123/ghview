debug:
    tmux new-session -d -s ghview-debug -x "$(tput cols)" -y "$(tput lines)"
    tmux send-keys -t ghview-debug "cargo run -- --debug" Enter
    tmux split-window -t ghview-debug -v -l 10
    tmux send-keys -t ghview-debug "tail -F debug.log" Enter
    tmux select-pane -t ghview-debug -U
    tmux attach-session -t ghview-debug

fmt:
    cargo fmt

lint:
    cargo fmt --check
    cargo clippy -- -D warnings

fix:
    cargo fmt
    cargo clippy --fix --allow-dirty

test:
    INSTA_UPDATE=no GH_CONFIG_DIR=/nonexistent-gh-config cargo test

# accept/update all insta snapshots non-interactively
update-snapshots:
    INSTA_UPDATE=always GH_CONFIG_DIR=/nonexistent-gh-config cargo test

# requires cargo-insta: cargo install cargo-insta
snapshots:
    cargo insta test --review

fixtures org="ratatui" org_repo="ratatui/ratatui" user="sindresorhus" user_repo="sindresorhus/got":
    GHVIEW_FIXTURE_ORG="{{org}}" GHVIEW_FIXTURE_ORG_REPO="{{org_repo}}" GHVIEW_FIXTURE_USER="{{user}}" GHVIEW_FIXTURE_USER_REPO="{{user_repo}}" ./scripts/capture-fixtures.sh
    GHVIEW_FIXTURE_ORGS="{{org}}" GHVIEW_FIXTURE_USER="{{user}}" python3 scripts/anonymize-fixtures.py
