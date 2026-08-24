//! Document model, loaders, edit ops, undo and view state for vuwr.
//!
//! This crate performs **no I/O** — it takes bytes and returns bytes, which is
//! what makes it portable to `wasm32-unknown-unknown` and to future native
//! mobile UIs. Do not add dependencies that touch the filesystem, threads
//! (`rayon`), `std::time::Instant` (use `web-time`), or `memmap2`. CI checks
//! this crate against the wasm target on every push.

// Phase 1 lands here: `Document::parse`, `EditOp`, undo, `serialize`.
