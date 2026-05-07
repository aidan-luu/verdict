//! Fetch FDA-hosted PDFs by URL with basic SSRF and abuse controls.
//!
//! Uses a dedicated `reqwest::Client` with redirect validation so a redirect
//! cannot escape the allowed host policy.

use std::net::IpAddr;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::CONTENT_TYPE;
use reqwest::redirect::Policy;
use reqwest::Url;

use crate::error::AppError;

const DEFAULT_MAX_BYTES: usize = 25 * 1024 * 1024;

/// Limits and host policy for PDF fetches (loaded from env in production).
#[derive(Debug, Clone)]
pub struct PdfFetchConfig {
    pub max_bytes: usize,
    pub allowed_host_suffixes: Vec<String>,
    /// When true, allows `http://127.0.0.1`, `http://localhost`, and `http://[::1]`
    /// for local stub servers in tests (never enable in production).
    pub allow_insecure_localhost: bool,
}

impl PdfFetchConfig {
    pub fn from_env() -> Result<Self, AppError> {
        let max_bytes = std::env::var("FDA_PDF_MAX_BYTES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_MAX_BYTES);

        let suffixes_raw = std::env::var("FDA_PDF_ALLOWED_HOST_SUFFIXES")
            .unwrap_or_else(|_| "fda.gov".to_string());

        let allowed_host_suffixes = suffixes_raw
            .split(',')
            .map(|piece| piece.trim().to_ascii_lowercase())
            .filter(|piece| !piece.is_empty())
            .collect::<Vec<_>>();

        if allowed_host_suffixes.is_empty() {
            return Err(AppError::BadRequest(
                "FDA_PDF_ALLOWED_HOST_SUFFIXES must list at least one host suffix".to_string(),
            ));
        }

        let allow_insecure_localhost = std::env::var("FDA_PDF_ALLOW_INSECURE_LOCALHOST")
            .ok()
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));

        Ok(Self {
            max_bytes,
            allowed_host_suffixes,
            allow_insecure_localhost,
        })
    }

    pub fn for_tests() -> Self {
        Self {
            max_bytes: 1024 * 1024,
            allowed_host_suffixes: vec!["fda.gov".to_string()],
            allow_insecure_localhost: true,
        }
    }
}

/// Build a client that only follows redirects to URLs passing [`validate_pdf_source_url`].
fn build_pdf_client(config: &PdfFetchConfig) -> Result<reqwest::Client, AppError> {
    let config = config.clone();
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .redirect(Policy::custom(move |attempt| {
            if attempt.previous().len() >= 8 {
                return attempt.stop();
            }
            if validate_pdf_source_url(attempt.url(), &config).is_ok() {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|_| AppError::Internal)
}

/// Validate URL scheme, host policy, and SSRF-shaped inputs before any network I/O.
pub fn validate_pdf_source_url(url: &Url, config: &PdfFetchConfig) -> Result<(), AppError> {
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::BadRequest(
            "pdf url must not include credentials".to_string(),
        ));
    }

    let host = url
        .host_str()
        .ok_or_else(|| AppError::BadRequest("pdf url must have a host".to_string()))?;

    match url.scheme() {
        "https" => match url.port() {
            None | Some(443) => {}
            Some(_) => {
                return Err(AppError::BadRequest(
                    "pdf url must use the default https port".to_string(),
                ));
            }
        },
        "http" => {
            if !config.allow_insecure_localhost {
                return Err(AppError::BadRequest("pdf url must use https".to_string()));
            }
            if !is_loopback_host(host) {
                return Err(AppError::BadRequest(
                    "insecure pdf fetch is only allowed for loopback hosts".to_string(),
                ));
            }
        }
        _ => {
            return Err(AppError::BadRequest("pdf url must use https".to_string()));
        }
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        if config.allow_insecure_localhost && ip.is_loopback() {
            return Ok(());
        }
        return Err(AppError::BadRequest(
            "pdf url must use a hostname, not an ip address".to_string(),
        ));
    }

    let host = host.to_ascii_lowercase();
    let allowed = config
        .allowed_host_suffixes
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")));

    if allowed {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "pdf url host is not in the allowed list".to_string(),
        ))
    }
}

fn is_loopback_host(host: &str) -> bool {
    host == "localhost"
        || host == "127.0.0.1"
        || host == "[::1]"
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Download a PDF from `url` after validation; enforces size cap and `%PDF` magic bytes.
pub async fn fetch_pdf_bytes(url: &str, config: &PdfFetchConfig) -> Result<Vec<u8>, AppError> {
    let parsed =
        Url::parse(url).map_err(|_| AppError::BadRequest("invalid pdf url".to_string()))?;
    validate_pdf_source_url(&parsed, config)?;

    let client = build_pdf_client(config)?;
    let response = client
        .get(parsed.clone())
        .send()
        .await
        .map_err(|_| AppError::BadRequest("could not download pdf".to_string()))?;

    if !response.status().is_success() {
        return Err(AppError::BadRequest(format!(
            "pdf fetch failed with status {}",
            response.status()
        )));
    }

    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    if content_type.contains("text/html") {
        return Err(AppError::BadRequest(
            "url returned html instead of a pdf".to_string(),
        ));
    }

    if let Some(length) = response.content_length() {
        if length > config.max_bytes as u64 {
            return Err(AppError::BadRequest(
                "pdf exceeds maximum allowed size".to_string(),
            ));
        }
    }

    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|_| AppError::BadRequest("pdf download interrupted".to_string()))?;
        if buffer.len().saturating_add(chunk.len()) > config.max_bytes {
            return Err(AppError::BadRequest(
                "pdf exceeds maximum allowed size".to_string(),
            ));
        }
        buffer.extend_from_slice(&chunk);
    }

    if !buffer.starts_with(b"%PDF") {
        return Err(AppError::BadRequest(
            "downloaded file is not a pdf".to_string(),
        ));
    }

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[test]
    fn validate_accepts_fda_host() {
        let url = Url::parse("https://www.fda.gov/foo/bar.pdf").expect("url");
        let config = PdfFetchConfig::for_tests();
        validate_pdf_source_url(&url, &config).expect("fda host should be allowed");
    }

    #[test]
    fn validate_rejects_non_allowed_host() {
        let url = Url::parse("https://example.com/x.pdf").expect("url");
        let config = PdfFetchConfig::for_tests();
        assert!(validate_pdf_source_url(&url, &config).is_err());
    }

    #[test]
    fn validate_rejects_ip_literals() {
        let url = Url::parse("https://203.0.113.1/x.pdf").expect("url");
        let mut config = PdfFetchConfig::for_tests();
        config.allow_insecure_localhost = false;
        assert!(validate_pdf_source_url(&url, &config).is_err());
    }

    #[tokio::test]
    async fn fetch_loads_pdf_from_loopback_stub() {
        let body = b"%PDF-1.4\n%stub\n".to_vec();
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let url = format!("http://127.0.0.1:{port}/test.pdf");

        tokio::spawn(async move {
            let mut socket = listener.accept().await.expect("accept").0;
            let mut buf = vec![0u8; 2048];
            let _ = tokio::time::timeout(Duration::from_secs(2), socket.read(&mut buf))
                .await
                .expect("read timeout")
                .expect("read");

            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/pdf\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let mut out = header.into_bytes();
            out.extend_from_slice(&body);
            socket.write_all(&out).await.expect("write");
        });

        tokio::time::sleep(Duration::from_millis(20)).await;

        let config = PdfFetchConfig::for_tests();
        let bytes = fetch_pdf_bytes(&url, &config).await.expect("fetch");
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[tokio::test]
    async fn fetch_rejects_oversized_content_length() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let url = format!("http://127.0.0.1:{port}/big.pdf");

        tokio::spawn(async move {
            let mut socket = listener.accept().await.expect("accept").0;
            let mut buf = vec![0u8; 2048];
            let _ = tokio::time::timeout(Duration::from_secs(2), socket.read(&mut buf))
                .await
                .expect("read timeout")
                .expect("read");

            let header = "HTTP/1.1 200 OK\r\nContent-Type: application/pdf\r\nContent-Length: 999999999\r\nConnection: close\r\n\r\n";
            socket.write_all(header.as_bytes()).await.expect("write");
        });

        tokio::time::sleep(Duration::from_millis(20)).await;

        let mut config = PdfFetchConfig::for_tests();
        config.max_bytes = 1024;
        let err = fetch_pdf_bytes(&url, &config).await.expect_err("too large");
        match err {
            AppError::BadRequest(message) => assert!(message.contains("size")),
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
