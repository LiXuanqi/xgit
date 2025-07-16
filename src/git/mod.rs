//! Git operations module
//!
//! This module provides a domain-driven structure for Git operations:
//!
//! - `repository`: Core repository operations (init, open, signatures)
//! - `branches`: Branch operations (create, checkout, list, tracking)
//! - `commits`: Commit operations (add, commit, diff, staged changes)
//! - `remotes`: Remote operations (add, push, fetch, pull)
//! - `merge`: Merge operations (merge strategies, pull merges)

pub mod branches;
pub mod commits;
pub mod merge;
pub mod remotes;
pub mod repository;

// Re-export the main types
pub use repository::core::GitRepo;
