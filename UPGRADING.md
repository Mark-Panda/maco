# UPGRADING (R25)

## Before bumping `adk-rust`

1. Check MSRV in `adk-rust` crate on crates.io
2. Run `cargo run -p maco-harness --example run_spike`
3. Run `cargo test --workspace`
4. Review adk session/memory embedded migration changes in release notes
5. Backup `~/.maco/data/` via `cargo run -p maco-server -- backup`

## Pin policy

`adk-rust = "=1.0.0"` in workspace `Cargo.toml` until spike passes on new version.
