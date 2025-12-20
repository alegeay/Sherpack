//! Standard exit codes for CLI operations
//!
//! These exit codes follow Unix conventions and sysexits.h where applicable.

#![allow(dead_code)]

/// Success - operation completed without errors
pub const SUCCESS: i32 = 0;

/// General error - unspecified failure
pub const ERROR: i32 = 1;

/// Validation error - schema or values validation failed
pub const VALIDATION_ERROR: i32 = 2;

/// Template error - template rendering failed
pub const TEMPLATE_ERROR: i32 = 3;

/// Pack error - invalid pack structure or Pack.yaml
pub const PACK_ERROR: i32 = 4;

/// IO error - file not found, permission denied, etc.
pub const IO_ERROR: i32 = 5;

/// Usage error - invalid arguments or options (following sysexits.h convention)
pub const USAGE_ERROR: i32 = 64;
