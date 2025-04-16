use std::collections::HashSet;

use crate::{client::EditorClient, map::EditorResources, notifications::EditorNotification};

pub fn check_legacy_resource_limit_images(client: &EditorClient, resources: &EditorResources) {
    let imgs: HashSet<_> = resources
        .images
        .iter()
        .map(|i| i.def.meta.blake3_hash)
        .chain(
            resources
                .image_arrays
                .iter()
                .map(|i| i.def.meta.blake3_hash),
        )
        .collect();
    // ddnet limitation
    if imgs.len() >= 64 {
        client.notifications.push(EditorNotification::Warning(
            "Adding more than 64 images makes \
        this map incompatible to (old) ddnet"
                .to_string(),
        ));
    }
}
