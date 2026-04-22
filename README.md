# copyforge

A COBOL copybook parser and binary record decoder written in Rust.

**Status:** early WIP — no features are usable yet.

**MSRV:** 1.80.0

## License

Dual-licensed under either of

- [MIT license](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

## Development

Requires Rust 1.80.0 and [just](https://github.com/casey/just).

Common tasks:

    just          # list recipes
    just test     # run tests
    just lint     # clippy, warnings-as-errors
    just pre-push # full local gate before pushing

Snapshot review (`just snap` / `just snap-accept`) additionally requires [cargo-insta](https://insta.rs):

    cargo install cargo-insta

