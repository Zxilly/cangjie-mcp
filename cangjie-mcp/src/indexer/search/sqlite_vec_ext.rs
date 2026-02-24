#![allow(dead_code)]

use std::ffi::{c_char, c_int};

use anyhow::{Context, Result};
use rusqlite::auto_extension::{register_auto_extension, RawAutoExtension};

#[link(name = "sqlite_vec0")]
unsafe extern "C" {
    #[link_name = "sqlite3_vec_init"]
    fn sqlite3_vec_init_raw(
        db: *mut rusqlite::ffi::sqlite3,
        pz_err_msg: *mut *mut c_char,
        p_api: *const rusqlite::ffi::sqlite3_api_routines,
    ) -> c_int;
}

/// Register sqlite-vec as SQLite auto extension.
///
/// Safe to call multiple times because SQLite de-duplicates auto-extensions.
pub(crate) fn register_sqlite_vec() -> Result<()> {
    let ext: RawAutoExtension = sqlite3_vec_init_raw;
    unsafe { register_auto_extension(ext).context("Failed to register sqlite-vec auto extension") }
}
