use std::path::Path;

use anyhow::anyhow;

/// Converts path components to a properly url encoded path string.
pub fn relative_path_to_url(path: &Path) -> anyhow::Result<String> {
    if path.is_absolute() {
        anyhow::bail!("Only relative paths are supported, but absolute path was found: {path:?}");
    }
    Ok(path
        .components()
        .map(|p| {
            p.as_os_str()
                .to_str()
                .map(|p| urlencoding::encode(p).to_string())
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow!("One or more components were not valid strings: {path:?}"))?
        .join("/"))
}
