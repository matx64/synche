dev:
    watchexec \
        --watch app \
        --watch gui/index.html \
        --ignore target \
        --restart \
        "cargo run -p synche"

setup-hooks:
    git config core.hooksPath .githooks
    @echo "Git hooks installed. Run 'just setup-hooks' once after cloning."
