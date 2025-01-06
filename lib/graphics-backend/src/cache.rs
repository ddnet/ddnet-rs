use std::sync::Arc;

use base_io_traits::fs_traits::FileSystemInterface;
use cache::Cache;

pub async fn get_backend_cache(fs: &Arc<dyn FileSystemInterface>) -> Cache<6012025> {
    Cache::new_async("graphics-backend-cache", fs).await
}
