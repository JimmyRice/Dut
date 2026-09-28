//! The single gateway for outbound HTTP.
//!
//! Every upstream call goes through [`OutboundHttpClient::fetch`], so each
//! one is logged the same way: when it starts, how long it took, what came
//! back, and how upstream wants it cached.

mod client;
mod freshness;

pub use client::{
    OutboundHttpClient, ProxyMode, UpstreamError, UpstreamRequest, UpstreamResponse, build,
};
