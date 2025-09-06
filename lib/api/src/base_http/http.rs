use std::sync::atomic::AtomicU64;

use async_trait::async_trait;
use base::hash::Hash;
use base_io::yield_now::{self};
use base_io_traits::http_traits::{HttpClientInterface, HttpError, HttpHeaderValue};
use bytes::Bytes;
use url::Url;

use crate::{read_result_from_host, upload_param};

unsafe extern "C" {
    fn api_download_text();
    fn api_download_binary();
    fn api_post_json();
}

#[derive(Debug)]
pub struct HttpClient {
    id: AtomicU64,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            id: Default::default(),
        }
    }
}

#[async_trait]
impl HttpClientInterface for HttpClient {
    async fn download_text(&self, url: Url) -> anyhow::Result<String, HttpError> {
        let mut res;
        let id = self.id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        loop {
            upload_param(0, url.clone());
            upload_param(1, id);
            unsafe {
                api_download_text();
            }
            res = read_result_from_host::<Option<Result<String, HttpError>>>();
            if res.is_some() {
                break;
            } else {
                yield_now::yield_now().await;
            }
        }
        res.unwrap()
    }

    async fn download_binary_secure(&self, _url: Url) -> anyhow::Result<Bytes, HttpError> {
        panic!("not implemented yet")
    }

    async fn download_binary(&self, url: Url, hash: &Hash) -> anyhow::Result<Bytes, HttpError> {
        let mut res;
        let id = self.id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        loop {
            upload_param(0, url.clone());
            upload_param(1, *hash);
            upload_param(2, id);
            unsafe {
                api_download_binary();
            }
            res = read_result_from_host::<Option<Result<Bytes, HttpError>>>();
            if res.is_some() {
                break;
            } else {
                yield_now::yield_now().await;
            }
        }
        res.unwrap()
    }

    async fn post_json(&self, url: Url, data: Vec<u8>) -> anyhow::Result<Vec<u8>, HttpError> {
        let mut res;
        let id = self.id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        loop {
            upload_param(0, url.clone());
            upload_param(1, data.clone());
            upload_param(2, id);
            unsafe {
                api_post_json();
            }
            res = read_result_from_host::<Option<Result<Vec<u8>, HttpError>>>();
            if res.is_some() {
                break;
            } else {
                yield_now::yield_now().await;
            }
        }
        res.unwrap()
    }

    async fn custom_request(
        &self,
        _url: Url,
        _headers: Vec<HttpHeaderValue>,
        _content: Option<Vec<u8>>,
    ) -> anyhow::Result<Bytes, HttpError> {
        panic!("this doesn't sound useful for modding, so not implemented yet")
    }
}
