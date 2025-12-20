//! Sherpack Convert - Helm chart to Sherpack pack converter
//!
//! This crate provides functionality to convert Helm charts to Sherpack packs,
//! transforming Go templates into idiomatic Jinja2 syntax.
//!
//! # Philosophy
//!
//! Sherpack Convert prioritizes **Jinja2 elegance** over Helm compatibility.
//! Instead of replicating Go template's quirky function-based syntax, we convert
//! to natural Jinja2 patterns:
//!
//! | Helm (Go template)              | Sherpack (Jinja2)                |
//! |---------------------------------|----------------------------------|
//! | `{{ index .Values.list 0 }}`    | `{{ values.list[0] }}`           |
//! | `{{ add 1 2 }}`                 | `{{ 1 + 2 }}`                    |
//! | `{{ ternary "a" "b" .X }}`      | `{{ "a" if x else "b" }}`        |
//! | `{{ printf "%s-%s" a b }}`      | `{{ a ~ "-" ~ b }}`              |
//! | `{{ coalesce .A .B "c" }}`      | `{{ a or b or "c" }}`            |
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//! use sherpack_convert::{convert, ConvertOptions, convert_with_options};
//!
//! // Simple conversion
//! let result = convert(
//!     Path::new("./my-helm-chart"),
//!     Path::new("./my-sherpack-pack"),
//! ).unwrap();
//!
//! println!("Converted {} files", result.converted_files.len());
//!
//! // Check for unsupported features
//! for warning in &result.warnings {
//!     if warning.severity == sherpack_convert::WarningSeverity::Unsupported {
//!         println!("Unsupported: {} - {}", warning.pattern, warning.message);
//!         if let Some(ref suggestion) = warning.suggestion {
//!             println!("  Alternative: {}", suggestion);
//!         }
//!     }
//! }
//!
//! // Conversion with options
//! let options = ConvertOptions {
//!     force: true,
//!     dry_run: false,
//!     verbose: true,
//! };
//!
//! let result = convert_with_options(
//!     Path::new("./helm-chart"),
//!     Path::new("./sherpack-pack"),
//!     options,
//! ).unwrap();
//! ```
//!
//! # Unsupported Features
//!
//! Some Helm features are intentionally not supported because they are
//! anti-patterns in a GitOps workflow:
//!
//! - **Crypto functions** (`genCA`, `genPrivateKey`, etc.)
//!   → Use cert-manager or external-secrets
//! - **Files API** (`.Files.Get`, `.Files.Glob`)
//!   → Embed content in values.yaml or use ConfigMaps
//! - **DNS lookups** (`getHostByName`)
//!   → Use explicit values for deterministic rendering
//! - **Random functions** (`randAlphaNum`, etc.)
//!   → Pre-generate values or use external-secrets

pub mod ast;
pub mod chart;
pub mod converter;
pub mod error;
pub mod macro_processor;
pub mod parser;
pub mod transformer;
pub mod type_inference;

// Re-exports
pub use converter::{ConversionResult, ConvertOptions, Converter, convert, convert_with_options};
pub use error::{ConversionWarning, ConvertError, Result, WarningCategory, WarningSeverity};
pub use type_inference::{InferredType, TypeContext, TypeHeuristics};
