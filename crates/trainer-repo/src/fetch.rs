use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use crate::error::{Result, RepoError};

/// Maximum JSON payload fetched from a repository (5 MiB).
pub const MAX_FETCH_BYTES: usize = 5 * 1024 * 1024;

/// Fetches UTF-8 text from an `http://`, `https://`, `file://`, or local filesystem path.
pub fn fetch_text(url: &str) -> Result<String> {
    let bytes = fetch_bytes(url)?;
    String::from_utf8(bytes).map_err(|e| RepoError::FetchFailed {
        url: url.to_string(),
        reason: format!("response is not valid UTF-8: {e}"),
    })
}

/// Fetches raw bytes from an `http://`, `https://`, `file://`, or local filesystem path.
pub fn fetch_bytes(url: &str) -> Result<Vec<u8>> {
    if url.starts_with("http://") || url.starts_with("https://") {
        fetch_http(url)
    } else if let Some(rest) = url.strip_prefix("file://") {
        let path = decode_file_url(rest);
        read_local_capped(&path, url)
    } else {
        read_local_capped(&PathBuf::from(url), url)
    }
}

fn fetch_http(url: &str) -> Result<Vec<u8>> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(20))
        .user_agent("freeplay/0.1")
        .build();

    let resp = agent.get(url).call().map_err(|e| RepoError::FetchFailed {
        url: url.to_string(),
        reason: e.to_string(),
    })?;

    let mut reader = resp.into_reader().take((MAX_FETCH_BYTES + 1) as u64);
    let mut buf = Vec::new();
    reader
        .read_to_end(&mut buf)
        .map_err(|e| RepoError::FetchFailed {
            url: url.to_string(),
            reason: e.to_string(),
        })?;

    if buf.len() > MAX_FETCH_BYTES {
        return Err(RepoError::PayloadTooLarge {
            url: url.to_string(),
            max_bytes: MAX_FETCH_BYTES,
        });
    }
    Ok(buf)
}

fn read_local_capped(path: &std::path::Path, original_url: &str) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path).map_err(|e| RepoError::FetchFailed {
        url: original_url.to_string(),
        reason: e.to_string(),
    })?;
    let mut buf = Vec::new();
    file.take((MAX_FETCH_BYTES + 1) as u64)
        .read_to_end(&mut buf)?;
    if buf.len() > MAX_FETCH_BYTES {
        return Err(RepoError::PayloadTooLarge {
            url: original_url.to_string(),
            max_bytes: MAX_FETCH_BYTES,
        });
    }
    Ok(buf)
}

fn decode_file_url(rest: &str) -> PathBuf {
    // Handle `file:///C:/path/to/index.json` on Windows and `file:///path` on Unix.
    let stripped = rest.strip_prefix("localhost").unwrap_or(rest);
    #[cfg(windows)]
    let normalized = {
        let s = stripped.trim_start_matches('/');
        s.replace("%20", " ")
    };
    #[cfg(not(windows))]
    let normalized = stripped.replace("%20", " ");
    PathBuf::from(normalized)
}
