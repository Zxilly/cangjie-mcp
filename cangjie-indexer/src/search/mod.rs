pub mod bm25;
pub mod fusion;
mod local;
mod remote;
mod sqlite_vec_ext;
pub mod synonyms;
pub mod vector;

use std::sync::{Arc, LazyLock};

use jieba_rs::Jieba;

pub use local::LocalSearchIndex;
pub use remote::RemoteSearchIndex;

/// Global Jieba instance shared across all search components.
pub static GLOBAL_JIEBA: LazyLock<Arc<Jieba>> = LazyLock::new(|| Arc::new(Jieba::new()));
