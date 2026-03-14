#![allow(dead_code)]

use anyhow::{Context, Result};
use rusqlite::auto_extension::{register_auto_extension, RawAutoExtension};
use sqlite_vec::sqlite3_vec_init;

/// Register sqlite-vec as SQLite auto extension.
///
/// Safe to call multiple times because SQLite de-duplicates auto-extensions.
pub(crate) fn register_sqlite_vec() -> Result<()> {
    let ext: RawAutoExtension = unsafe { std::mem::transmute(sqlite3_vec_init as *const ()) };
    unsafe { register_auto_extension(ext).context("Failed to register sqlite-vec auto extension") }
}
