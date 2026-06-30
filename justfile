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
