//! STDIO Protocol Parser Implementations
//!
//! This module contains concrete implementations of the `StdioProtocolParser` trait.
//!
//! Currently available parsers:
//! - `StandardStdioParser`: The standard STDIO protocol parser (default)
//!
//! Future parsers may include YAML variants, TOML variants, etc.

mod standard;

pub use standard::StandardStdioParser;
