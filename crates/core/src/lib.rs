//! Core parsing and intermediate-representation layer for copyforge.
//!
//! This crate owns the pipeline from raw copybook text to a typed IR:
//! lexer, parser, resolver (for `COPY ... REPLACING`), semantic lowering,
//! and the stable IR that every code generator consumes. It has no
//! dependency on any codegen target, so downstream analysis tools can
//! depend on it without pulling in TypeScript, Python, or Rust emitters.

pub mod span;
