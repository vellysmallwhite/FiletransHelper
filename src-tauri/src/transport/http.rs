use std::time::Duration;

use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};

use crate::device::manager::DeviceEndpoint;
use crate::transfer::manager::FileUploadQuery;
use crate::transport::{Transport, TransportPing};

#[derive(Clone, Debug)]
pub struct HttpTransport {
    client: Client,
}

impl Default for HttpTransport {
    fn default() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(3))
            .no_proxy()
            .build()
            .expect("ZeroDrop HTTP client configuration should be valid");

        Self { client }
    }
}

impl HttpTransport {
    pub async fn ping(&self, endpoint: &DeviceEndpoint) -> Result<TransportPing, String> {
        let url = format!("http://{}:{}/api/ping", endpoint.host, endpoint.port);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|err| format!("ping {url} failed: {err}"))?;

        let status = response.status();
        if !status.is_success() {
            return Err(format!("ping {url} returned HTTP {status}"));
        }

        response
            .json::<TransportPing>()
            .await
            .map_err(|err| format!("parse ping response from {url} failed: {err}"))
    }

    pub async fn post_json<TRequest, TResponse>(
        &self,
        endpoint: &DeviceEndpoint,
        path: &str,
        payload: &TRequest,
    ) -> Result<TResponse, String>
    where
        TRequest: Serialize + ?Sized,
        TResponse: DeserializeOwned,
    {
        let url = endpoint_url(endpoint, path);
        let response = self
            .client
            .post(&url)
            .json(payload)
            .send()
            .await
            .map_err(|err| format!("POST {url} failed: {err}"))?;

        parse_json_response(response, &url).await
    }

    pub async fn put_bytes<TResponse>(
        &self,
        endpoint: &DeviceEndpoint,
        path: &str,
        bytes: Vec<u8>,
    ) -> Result<TResponse, String>
    where
        TResponse: DeserializeOwned,
    {
        let url = endpoint_url(endpoint, path);
        let response = self
            .client
            .put(&url)
            .body(bytes)
            .send()
            .await
            .map_err(|err| format!("PUT {url} failed: {err}"))?;

        parse_json_response(response, &url).await
    }

    pub async fn post_file_upload<TResponse>(
        &self,
        endpoint: &DeviceEndpoint,
        path: &str,
        query: &FileUploadQuery,
        bytes: Vec<u8>,
    ) -> Result<TResponse, String>
    where
        TResponse: DeserializeOwned,
    {
        let url = endpoint_url(endpoint, path);
        let response = self
            .client
            .post(&url)
            .query(query)
            .timeout(Duration::from_secs(600))
            .body(bytes)
            .send()
            .await
            .map_err(|err| format!("POST {url} failed: {err}"))?;

        parse_json_response(response, &url).await
    }
}

fn endpoint_url(endpoint: &DeviceEndpoint, path: &str) -> String {
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    format!("http://{}:{}{}", endpoint.host, endpoint.port, path)
}

async fn parse_json_response<TResponse>(
    response: reqwest::Response,
    url: &str,
) -> Result<TResponse, String>
where
    TResponse: DeserializeOwned,
{
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let suffix = if body.trim().is_empty() {
            String::new()
        } else {
            format!(": {body}")
        };
        return Err(format!("{url} returned HTTP {status}{suffix}"));
    }

    response
        .json::<TResponse>()
        .await
        .map_err(|err| format!("parse JSON response from {url} failed: {err}"))
}

impl Transport for HttpTransport {
    fn name(&self) -> &'static str {
        "http"
    }
}
