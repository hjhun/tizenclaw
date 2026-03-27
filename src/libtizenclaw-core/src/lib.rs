//! libtizenclaw-core — Plugin SDK for TizenClaw.
//!
//! Provides C FFI functions for:
//! - LLM data type handles (messages, tools, responses)
//! - HTTP helper (curl-like API backed by ureq)
//!
//! External plugins (.so) link against this library
//! and export symbols defined in the C headers.

#![allow(unused)]

pub mod llm_types;
pub mod curl_wrapper;
