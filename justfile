format:
    cargo fmt
    fd --extension nix --type file --exec-batch nix fmt -- {}
    shfmt --write .

format-check:
    cargo fmt --check
    fd --extension nix --type file --exec-batch nix fmt -- --check {}
    shfmt --diff .
