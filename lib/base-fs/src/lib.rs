pub mod filesys;

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use base_io::io::IoFileSys;
    use base_io_traits::fs_traits::{FileSystemInterface, FileSystemPath, FileSystemType};

    use crate::filesys::FileSystem;

    fn create_fs() -> (FileSystem, tokio::runtime::Runtime) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4) // should be at least 4
            .enable_all()
            .build()
            .unwrap();

        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
        std::env::set_current_dir(workspace_root).unwrap();
        (
            FileSystem::new(&rt, "ddnet-test", "ddnet-test", "ddnet-test", "ddnet-test").unwrap(),
            rt,
        )
    }

    #[test]
    fn read_dir_recursive() {
        let (fs, rt) = create_fs();

        let files = rt
            .block_on(fs.files_in_dir_recursive("skins/default".as_ref()))
            .unwrap();
        assert!(!files.is_empty());
        for (path, _) in files {
            println!("{:?}", path.to_string_lossy());
        }
    }

    #[test]
    fn entries_in_dir() {
        let (fs, rt) = create_fs();

        let files = rt
            .block_on(fs.entries_in_dir("skins/default".as_ref()))
            .unwrap();
        assert!(!files.is_empty());
        for path in files {
            println!("{:?}", path);
        }
    }

    #[test]
    fn entries_in_dir_weird() {
        let (fs, rt) = create_fs();

        let files = rt
            .block_on(fs.entries_in_dir("skins/default/../../skins/default".as_ref()))
            .unwrap();
        assert!(!files.is_empty());
        for path in files {
            println!("{:?}", path);
        }
    }

    #[test]
    fn read_file() {
        let (fs, rt) = create_fs();

        assert!(rt
            .block_on(fs.read_file("skins/default/body.png".as_ref()))
            .is_ok());
    }

    #[test]
    fn read_file_weird() {
        let (fs, rt) = create_fs();

        assert!(rt
            .block_on(fs.read_file("skins/default/../../skins/default/body.png".as_ref()))
            .is_ok());
    }

    #[test]
    fn read_file_in() {
        let (fs, rt) = create_fs();

        assert!(rt
            .block_on(fs.read_file_in(
                "skins/default/body.png".as_ref(),
                FileSystemPath::OfType(FileSystemType::Read)
            ))
            .is_ok());
    }

    #[test]
    fn create_dir_write_file_file_exists() {
        let (fs, rt) = create_fs();

        assert!(rt
            .block_on(fs.create_dir("test/test2/test3".as_ref()))
            .is_ok());
        // creating twice is ok
        assert!(rt
            .block_on(fs.create_dir("test/test2/test3".as_ref()))
            .is_ok());

        let file_res = rt.block_on(fs.read_file("skins/default/body.png".as_ref()));
        assert!(file_res.is_ok());
        let file = file_res.unwrap();

        assert!(rt
            .block_on(fs.write_file("test/test2/test3/file.png".as_ref(), file.clone()))
            .is_ok());
        // writing twice is ok
        assert!(rt
            .block_on(fs.write_file("test/test2/test3/file.png".as_ref(), file))
            .is_ok());

        assert!(rt.block_on(fs.file_exists("test/test2/test3/file.png".as_ref())));

        let files = rt
            .block_on(fs.files_in_dir_recursive("test/".as_ref()))
            .unwrap();
        assert!(!files.is_empty());
        for (path, _) in files {
            println!("{:?}", path.to_string_lossy());
        }
    }

    /*
    ouch
    #[cfg(target_os = "linux")]
    #[test]
    fn sym_link() {
        let (fs, rt) = create_fs();

        assert!(rt
            .block_on(fs.create_dir("test/test2/test3".as_ref()))
            .is_ok());

        let path = fs.get_save_path();
        let _ = std::os::unix::fs::symlink("/bin", path.join("escape"));

        let files = rt.block_on(fs.entries_in_dir("escape".as_ref())).unwrap();
        assert!(files.len() == 0);
    }*/

    #[test]
    fn runtime_panic_free() {
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
        std::env::set_current_dir(workspace_root).unwrap();
        let io = IoFileSys::new(|rt| {
            Arc::new(
                FileSystem::new(rt, "ddnet-test", "ddnet-test", "ddnet-test", "ddnet-test")
                    .unwrap(),
            )
        });

        assert!(io
            .rt
            .spawn::<(), _>(async {
                panic!("test");
            })
            .get_catch()
            .is_err());
    }
}
