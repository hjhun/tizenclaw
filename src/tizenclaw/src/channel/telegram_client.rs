//! Telegram Bot API client — async long-polling channel.
//!
//! Uses `getUpdates` long-polling to receive messages. Polls natively
//! on the Tokio async reactor (epoll) avoiding expensive thread allocation.

use super::{split_message_chunks, Channel, ChannelConfig};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncBufReadExt;

const MAX_CONCURRENT_HANDLERS: i32 = 3;
const DEFAULT_CLI_TIMEOUT_SECS: u64 = 900;
const TELEGRAM_CHAT_ACTION_UPDATE_SECS: u64 = 4;
const CLI_PROGRESS_UPDATE_SECS: u64 = 15;
const CLI_PROGRESS_MIN_PARTIAL_CHARS: usize = 80;
const DEFAULT_GEMINI_CLI_MODEL: &str = "gemini-2.5-flash";
const TELEGRAM_MAX_MESSAGE_CHARS: usize = 4000;

include!("telegram_client/types.rs");
include!("telegram_client/client.rs");
include!("telegram_client/client_impl.rs");
include!("telegram_client/transport.rs");
include!("telegram_client/commands.rs");
include!("telegram_client/execution.rs");
include!("telegram_client/channel_impl.rs");
include!("telegram_client/tests.rs");
