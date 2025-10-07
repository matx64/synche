dev:
    watchexec \
        --watch src \
        --watch gui/index.html \
        --ignore target \
        --restart \
        "cargo run"