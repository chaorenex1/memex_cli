//! Input Processing Module
//!
//! Provides unified input parsing logic that supports both:
//! - Structured STDIO protocol text (multi-task with dependencies)
//! - Plain text (auto-wrapped as single task)
//!
//! This module bridges the gap between CLI input and STDIO task execution.

mod parser;

pub use parser::InputParser;
