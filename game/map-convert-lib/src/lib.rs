pub mod legacy_to_new;
pub mod new_to_legacy;

#[cfg(test)]
mod test {
    use std::path::Path;
    use std::sync::Arc;

    use base_fs::filesys::FileSystem;
    use base_io::io::IoFileSys;

    use crate::legacy_to_new::{legacy_to_new, legacy_to_new_from_buf};
    use crate::new_to_legacy::new_to_legacy_from_buf_async;

    fn convert_back_and_forth_for_map(io: &IoFileSys, tp: &Arc<rayon::ThreadPool>, path: &Path) {
        let map_name = path.file_stem().unwrap().to_str().unwrap();
        println!("converting map: {map_name}");
        let new_map = legacy_to_new(path, io, tp, false).unwrap();
        let mut map = new_map.map.clone();

        let tp_task = tp.clone();
        let old_map = io
            .rt
            .spawn(async move {
                let file = new_map.map.write(&tp_task)?;
                new_to_legacy_from_buf_async(
                    &file,
                    |_| {
                        Box::pin(async move {
                            Ok((
                                new_map
                                    .map
                                    .resources
                                    .images
                                    .iter()
                                    .map(|i| {
                                        new_map
                                            .resources
                                            .images
                                            .get(&i.meta.blake3_hash)
                                            .map(|i| i.buf.clone())
                                            .unwrap()
                                    })
                                    .collect(),
                                new_map
                                    .map
                                    .resources
                                    .image_arrays
                                    .iter()
                                    .map(|i| {
                                        new_map
                                            .resources
                                            .images
                                            .get(&i.meta.blake3_hash)
                                            .map(|i| i.buf.clone())
                                            .unwrap()
                                    })
                                    .collect(),
                                new_map
                                    .map
                                    .resources
                                    .sounds
                                    .iter()
                                    .map(|s| {
                                        new_map
                                            .resources
                                            .sounds
                                            .get(&s.meta.blake3_hash)
                                            .map(|s| s.buf.clone())
                                            .unwrap()
                                    })
                                    .collect(),
                            ))
                        })
                    },
                    &tp_task,
                )
                .await
            })
            .get()
            .unwrap();

        let new_map2 = legacy_to_new_from_buf(old_map.map, map_name, io, tp, false).unwrap();
        let mut map2 = new_map2.map;

        fn assert_json_eq<A: serde::Serialize, B: serde::Serialize>(
            map_name: &str,
            name: &str,
            a: &A,
            b: &B,
        ) {
            let map1_json = serde_json::to_string_pretty(a).unwrap();
            let map2_json = serde_json::to_string_pretty(b).unwrap();
            let found_diff = map1_json
                .chars()
                .zip(map2_json.chars())
                .enumerate()
                .find(|(_, (char1, char2))| char1.ne(char2));
            if let Some((diff_index, _)) = found_diff {
                let range_len = 80;
                let s1_start = diff_index.max(range_len) - range_len;
                let s1_end = s1_start + (map1_json.len() - s1_start).min(range_len * 2);

                let s2_start = diff_index.max(range_len) - range_len;
                let s2_end = s2_start + (map2_json.len() - s2_start).min(range_len * 2);

                let diff = difference::Changeset::new(
                    &String::from_utf8_lossy(&map1_json.as_bytes()[s1_start..s1_end]),
                    &String::from_utf8_lossy(&map2_json.as_bytes()[s2_start..s2_end]),
                    "\n",
                );

                panic!(
                    "difference found for {map_name} {name} @{diff_index}: \n{}\n in\n{} vs.\n{}",
                    diff,
                    &String::from_utf8_lossy(&map1_json.as_bytes()[s1_start..s1_end]),
                    &String::from_utf8_lossy(&map2_json.as_bytes()[s2_start..s2_end]),
                );
            }
            assert!(
                map1_json.len() == map2_json.len(),
                "{name} did not match by length"
            );
        }

        // ignore sounds for now, since the hash always changes
        map.resources.sounds.clear();
        map2.resources.sounds.clear();

        // animation
        assert_json_eq(map_name, "animations", &map.animations, &map2.animations);
        assert_json_eq(map_name, "resources", &map.resources, &map2.resources);
        assert_json_eq(
            map_name,
            "bg groups",
            &map.groups.background,
            &map2.groups.background,
        );
        assert_json_eq(
            map_name,
            "physics groups",
            &map.groups.physics,
            &map2.groups.physics,
        );
        assert_json_eq(
            map_name,
            "fg groups",
            &map.groups.foreground,
            &map2.groups.foreground,
        );
    }

    #[test]
    fn convert_back_and_forth() {
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
        std::env::set_current_dir(workspace_root).unwrap();
        let io = IoFileSys::new(|rt| {
            Arc::new(
                FileSystem::new(rt, "ddnet-test", "ddnet-test", "ddnet-test", "ddnet-test")
                    .unwrap(),
            )
        });
        let thread_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(1)
                .build()
                .unwrap(),
        );
        let in_ty = |ty: &str| {
            let fs = io.fs.clone();
            let ty_t = ty.to_string();
            let entries = io
                .rt
                .spawn(async move {
                    Ok(fs
                        .entries_in_dir(format!("types/{ty_t}/maps").as_ref())
                        .await
                        .unwrap())
                })
                .get()
                .unwrap();

            for (path, _) in entries {
                if path.ends_with(".map") {
                    let path: &std::path::Path = path.as_ref();
                    convert_back_and_forth_for_map(&io, &thread_pool, path);
                }
            }
        };
        in_ty("novice");
        in_ty("moderate");
        in_ty("brutal");
        in_ty("ddmax.easy");
        in_ty("ddmax.next");
        in_ty("ddmax.nut");
        in_ty("ddmax.pro");
        in_ty("dummy");
        in_ty("fun");
        in_ty("insane");
        in_ty("race");
        in_ty("solo");
    }
}
