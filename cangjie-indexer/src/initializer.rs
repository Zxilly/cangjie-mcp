mod build;
mod prebuilt;

use anyhow::{Context, Result};
use tracing::info;

use cangjie_core::config::{IndexInfo, Settings};

use build::build_index;
use prebuilt::{index_is_ready, load_prebuilt_index};

/// Initialize repository and build index if needed.
pub async fn initialize_and_index(settings: &Settings) -> Result<IndexInfo> {
    if settings.prebuilt.is_prebuilt() {
        return load_prebuilt_index(settings).await;
    }

    use crate::repo::GitManager;

    // Resolve versions concurrently (ensures repos are cloned, fetched, and checked out)
    let mut git_mgr = GitManager::new(
        settings.docs_repo_dir(),
        cangjie_core::config::DOCS_REPO_URL.to_string(),
    );
    let mut runtime_mgr = GitManager::new(
        settings.runtime_repo_dir(),
        cangjie_core::config::RUNTIME_REPO_URL.to_string(),
    );
    let mut stdx_mgr = GitManager::new(
        settings.stdx_repo_dir(),
        cangjie_core::config::STDX_REPO_URL.to_string(),
    );

    let (docs_result, runtime_result, stdx_result) = tokio::join!(
        git_mgr.resolve_version(&settings.docs_version),
        runtime_mgr.resolve_version(&settings.runtime_version),
        stdx_mgr.resolve_version(&settings.stdx_version),
    );
    let resolved_version = docs_result.context("Failed to resolve documentation version")?;
    let runtime_resolved =
        runtime_result.context("Failed to resolve runtime documentation version")?;
    let stdx_resolved = stdx_result.context("Failed to resolve stdx documentation version")?;
    info!(
        "Resolved docs version: {} -> {}",
        settings.docs_version, resolved_version
    );
    info!(
        "Resolved runtime version: {} -> {}",
        settings.runtime_version, runtime_resolved
    );
    info!(
        "Resolved stdx version: {} -> {}",
        settings.stdx_version, stdx_resolved
    );

    let combined_version = format!("{resolved_version}+rt-{runtime_resolved}+stdx-{stdx_resolved}");
    let index_info = IndexInfo::from_settings(settings, &combined_version);

    if index_is_ready(&index_info).await {
        info!(
            "Index already exists (version: {}, lang: {})",
            resolved_version, settings.docs_lang
        );
        return Ok(index_info);
    }

    build_index(settings, &index_info).await?;

    Ok(index_info)
}
