use std::{io::Write, path::Path, sync::Arc};

use anyhow::anyhow;
use base_io_traits::fs_traits::FileSystemInterface;

/// Editor supports global paths, that's why this should be used
pub async fn read_file_editor(
    fs: &Arc<dyn FileSystemInterface>,
    path: &Path,
) -> anyhow::Result<Vec<u8>> {
    if path.is_absolute() {
        Ok(tokio::fs::read(path).await?)
    } else {
        Ok(fs.read_file(path).await?)
    }
}

/// Editor supports global paths, that's why this should be used
pub async fn write_file_editor(
    fs: &Arc<dyn FileSystemInterface>,
    path: &Path,
    data: Vec<u8>,
) -> anyhow::Result<()> {
    if path.is_absolute() {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow!("file path had no parent directory."))?
            .to_path_buf();
        let file = tokio::task::spawn_blocking(move || {
            let mut file = tempfile::NamedTempFile::new_in(parent)?;
            file.write_all(&data)?;
            file.flush()?;
            Ok::<_, std::io::Error>(file)
        })
        .await??;
        let (_, temp_file_path) = file.keep()?;
        Ok(tokio::fs::rename(temp_file_path, path).await?)
    } else {
        Ok(fs.write_file(path, data).await?)
    }
}
