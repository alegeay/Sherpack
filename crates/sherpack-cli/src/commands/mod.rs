//! CLI commands

pub mod template;
pub mod create;
pub mod lint;
pub mod show;
pub mod validate;
pub mod package;
pub mod inspect;
pub mod keygen;
pub mod sign;
pub mod signing;
pub mod verify;
pub mod convert;

// Phase 4 - Kubernetes deployment commands
pub mod install;
pub mod upgrade;
pub mod uninstall;
pub mod rollback;
pub mod list;
pub mod history;
pub mod status;
pub mod recover;

// Phase 5 - Repository management
pub mod repo;
pub mod search;
pub mod pull;
pub mod push;
pub mod dep;
