# List available recipes
default:
    @just --list

# Format all Rust code
fmt:
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Clippy on the full workspace, warnings as errors
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Run all tests
test:
    cargo test --workspace

# Review pending insta snapshots
snap:
    cargo insta review

# Accept all pending snapshots without review — use sparingly
snap-accept:
    cargo insta accept

# Run the identity-concealment check on staged files
check-identity:
    ./.claude/check-identity.sh

# Full local gate — run before every push
pre-push: fmt-check lint test
    ./.claude/check-identity.sh --range="$(git rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2>/dev/null || echo origin/main)..HEAD"
    @echo "pre-push: all checks passed"

# Clean build artifacts
clean:
    cargo clean
