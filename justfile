dev:
    watchexec \
        --watch app \
        --watch gui/index.html \
        --ignore target \
        --restart \
        "cargo run -p synche"