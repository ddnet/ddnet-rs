use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

use base_io::path_to_url::relative_path_to_url;
use tokio::net::TcpSocket;
use tower_http::services::ServeDir;

/// this server is only intended for file downloads
/// e.g. downloading images, wasm modules etc.
pub struct HttpDownloadServer {
    rt: Option<tokio::runtime::Runtime>,
    join: Vec<tokio::task::JoinHandle<anyhow::Result<()>>>,

    pub port_v4: u16,
    pub port_v6: u16,
}

impl HttpDownloadServer {
    pub fn new(
        served_files: HashMap<String, Vec<u8>>,
        served_dirs_disk: HashMap<String, PathBuf>,
        ipv4_port: u16,
        ipv6_port: u16,
    ) -> anyhow::Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()?;
        let _g = rt.enter();

        let start_http_server =
            |tcp_socket: TcpSocket,
             served_files: HashMap<String, Vec<u8>>,
             served_dirs_disk: HashMap<String, PathBuf>| {
                let addr = tcp_socket.local_addr()?;
                let listener = tcp_socket.listen(1024).unwrap();

                anyhow::Ok((
                    tokio::task::spawn(async move {
                        // build our application with a single route
                        let mut app = axum::Router::new();

                        for (name, served_file) in served_files {
                            let path: &Path = name.as_ref();
                            let path = match relative_path_to_url(path) {
                                Ok(path) => path,
                                Err(err) => {
                                    log::warn!("Found file with unsupported absolute path: {name}");
                                    anyhow::bail!(err);
                                }
                            };
                            app = app.route(
                                &format!("/{}", path),
                                axum::routing::get(|| async move { served_file }),
                            );
                        }
                        for (served_dir_path, served_dir) in served_dirs_disk {
                            app = app.nest_service(
                                &format!("/{served_dir_path}"),
                                ServeDir::new(served_dir),
                            )
                        }

                        axum::serve(listener, app).await?;
                        Ok(())
                    }),
                    addr.port(),
                ))
            };

        let tcp_socket = TcpSocket::new_v4()?;
        tcp_socket.set_reuseaddr(true)?;
        tcp_socket.bind(format!("0.0.0.0:{}", ipv4_port).parse()?)?;
        let (join_v4, port_v4) =
            start_http_server(tcp_socket, served_files.clone(), served_dirs_disk.clone())?;

        let tcp_socket = TcpSocket::new_v6()?;
        tcp_socket.set_reuseaddr(true)?;
        tcp_socket.bind(format!("[::0]:{}", ipv6_port).parse()?)?;
        let (join_v6, port_v6) =
            start_http_server(tcp_socket, served_files.clone(), served_dirs_disk.clone())?;
        Ok(Self {
            rt: Some(rt),
            join: vec![join_v4, join_v6],

            port_v4,
            port_v6,
        })
    }
}

impl Drop for HttpDownloadServer {
    fn drop(&mut self) {
        if let Some(rt) = self.rt.take() {
            for join in self.join.drain(..) {
                join.abort();
                if let Ok(Err(err)) = rt.block_on(join) {
                    log::error!("http server exited with an error: {err}");
                }
            }
            rt.shutdown_timeout(Duration::from_secs(1));
        }
    }
}
