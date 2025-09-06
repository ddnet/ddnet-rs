use std::net::IpAddr;
use std::{str::FromStr, sync::Arc};

use async_trait::async_trait;
use base::hash::Hash;
use base::hash::generate_hash_for;
use base_io_traits::http_traits::HttpError;
use base_io_traits::http_traits::{HttpClientInterface, HttpHeaderValue};
use bytes::Bytes;
use http_body_util::{BodyExt, Limited};
use reqwest::header::CONTENT_TYPE;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use tokio::sync::Mutex;
use url::Url;

#[derive(Debug, Clone)]
pub struct HttpClient {
    client: reqwest::Client,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::ClientBuilder::new().build().unwrap(),
        }
    }

    /// Automatically fallsback to ignoring the addr if the client could
    /// not be built.
    pub fn new_with_bind_addr(addr: IpAddr) -> Option<Self> {
        reqwest::ClientBuilder::new()
            .local_address(addr)
            .build()
            .ok()
            .map(|client| Self { client })
    }

    fn map_err<R>(r: Result<R, reqwest::Error>) -> Result<R, HttpError> {
        r.map_err(|err| {
            if err.is_request() {
                HttpError::Request
            } else if err.is_status() {
                HttpError::Status(err.status().unwrap().as_u16())
            } else {
                HttpError::Other(err.to_string())
            }
        })
    }

    async fn to_bytes_limited(r: reqwest::Response) -> Result<Bytes, HttpError> {
        let res = http::Response::from(r);
        let limited = Limited::new(res, 1024 * 1024 * 1024);
        Ok(limited
            .collect()
            .await
            .map_err(|err| HttpError::Other(err.to_string()))?
            .to_bytes())
    }
}

#[async_trait]
impl HttpClientInterface for HttpClient {
    async fn download_text(&self, url: Url) -> anyhow::Result<String, HttpError> {
        (url.scheme() == "https")
            .then_some(())
            .ok_or_else(|| HttpError::Other("url must be https".into()))?;
        let res = Self::to_bytes_limited(Self::map_err(self.client.get(url).send().await)?).await?;
        String::from_utf8(res.to_vec()).map_err(|err| HttpError::Other(err.to_string()))
    }

    async fn download_binary_secure(&self, url: Url) -> anyhow::Result<Bytes, HttpError> {
        (url.scheme() == "https")
            .then_some(())
            .ok_or_else(|| HttpError::Other("url must be https".into()))?;
        let res = Self::map_err(self.client.get(url).send().await)?;
        Self::to_bytes_limited(res).await
    }

    async fn download_binary(&self, url: Url, hash: &Hash) -> anyhow::Result<Bytes, HttpError> {
        (url.scheme() == "https" || url.scheme() == "http")
            .then_some(())
            .ok_or_else(|| HttpError::Other("url must be http or https".into()))?;
        let res = Self::to_bytes_limited(Self::map_err(self.client.get(url.clone()).send().await)?)
            .await?;

        generate_hash_for(&res)
            .eq(hash)
            .then_some(())
            .ok_or_else(|| HttpError::Other(format!("file hash mismatched for {url}")))?;

        Ok(res)
    }

    async fn post_json(&self, url: Url, data: Vec<u8>) -> anyhow::Result<Vec<u8>, HttpError> {
        if url.scheme() != "https" {
            return Err(HttpError::Other("url must be http or https".to_string()));
        };

        let res = Self::map_err(
            self.client
                .post(url)
                .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
                .body(data)
                .send()
                .await,
        )?;
        Ok(Self::map_err(res.bytes().await)?.to_vec())
    }

    async fn custom_request(
        &self,
        url: Url,
        headers: Vec<HttpHeaderValue>,
        content: Option<Vec<u8>>,
    ) -> anyhow::Result<Bytes, HttpError> {
        (url.scheme() == "https")
            .then_some(())
            .ok_or_else(|| HttpError::Other("url must be https".into()))?;
        let req = self.client.post(url);

        let mut http_headers = HeaderMap::default();
        for req_header in headers {
            match req_header {
                HttpHeaderValue::String { name, value } => {
                    http_headers.append(
                        HeaderName::from_str(&name)
                            .map_err(|err| HttpError::Other(err.to_string()))?,
                        HeaderValue::from_str(&value)
                            .map_err(|err| HttpError::Other(err.to_string()))?,
                    );
                }
                HttpHeaderValue::Int { name, value } => {
                    http_headers.append(
                        HeaderName::from_str(&name)
                            .map_err(|err| HttpError::Other(err.to_string()))?,
                        HeaderValue::from_str(&format!("{value}"))
                            .map_err(|err| HttpError::Other(err.to_string()))?,
                    );
                }
            }
        }

        let mut req = req.headers(http_headers);
        if let Some(body) = content {
            req = req.body(body);
        }
        let res = Self::map_err(Self::map_err(req.send().await)?.bytes().await)?;

        Ok(res)
    }
}

/// a wrapper around [`HttpClient`] that only initializes the http client once it's used (and also initializes it async)
#[derive(Debug, Clone)]
pub struct LazyHttpClient {
    client: Arc<Mutex<Option<HttpClient>>>,
}

impl Default for LazyHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyHttpClient {
    pub fn new() -> Self {
        Self {
            client: Default::default(),
        }
    }

    // get a clone of the underlaying client, if not initialized yet this function will do so.
    pub async fn get_client(&self) -> HttpClient {
        let mut client = self.client.lock().await;
        if client.is_none() {
            *client = Some(HttpClient::new());
        }
        client.clone().unwrap()
    }
}

#[async_trait]
impl HttpClientInterface for LazyHttpClient {
    async fn download_text(&self, url: Url) -> anyhow::Result<String, HttpError> {
        self.get_client().await.download_text(url).await
    }

    async fn download_binary_secure(&self, url: Url) -> anyhow::Result<Bytes, HttpError> {
        self.get_client().await.download_binary_secure(url).await
    }

    async fn download_binary(&self, url: Url, hash: &Hash) -> anyhow::Result<Bytes, HttpError> {
        self.get_client().await.download_binary(url, hash).await
    }

    async fn post_json(&self, url: Url, data: Vec<u8>) -> anyhow::Result<Vec<u8>, HttpError> {
        self.get_client().await.post_json(url, data).await
    }

    async fn custom_request(
        &self,
        url: Url,
        headers: Vec<HttpHeaderValue>,
        content: Option<Vec<u8>>,
    ) -> anyhow::Result<Bytes, HttpError> {
        self.get_client()
            .await
            .custom_request(url, headers, content)
            .await
    }
}

#[cfg(test)]
mod test {
    use base::benchmark::Benchmark;

    use super::{HttpClient, LazyHttpClient};

    #[test]
    fn http_create_bench() {
        let benchmark = Benchmark::new(true);
        let client = HttpClient::new();
        benchmark.bench("client 1");
        let client2 = HttpClient::new();
        benchmark.bench("client 2");
        drop(client);
        drop(client2);
        let client = LazyHttpClient::new();
        benchmark.bench("client 1 lazy");
        let client2 = LazyHttpClient::new();
        benchmark.bench("client 2 lazy");
        drop(client);
        drop(client2);
    }
}
