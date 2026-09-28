//! The MTR domain and the use cases built on it.
//!
//! `domain` holds pure business types and rules, and `application` holds the
//! use cases with the ports through which they read external data. Neither
//! depends on a web framework, an HTTP client, or a serialisation library, so
//! every other crate in the workspace can build on them.

pub mod application;
pub mod domain;
