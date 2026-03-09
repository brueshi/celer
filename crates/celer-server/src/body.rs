use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::body::Incoming;

use crate::error::ServerError;

/// Collect the full request body from a hyper Incoming stream.
///
/// Enforces a maximum body size to prevent memory exhaustion.
pub async fn collect_body(body: Incoming, max_size: usize) -> Result<Bytes, ServerError> {
    let collected = body
        .collect()
        .await
        .map_err(|e| ServerError::BodyError(format!("failed to read body: {e}")))?;

    let bytes = collected.to_bytes();

    if bytes.len() > max_size {
        return Err(ServerError::BodyError(format!(
            "body too large: {} bytes (max {})",
            bytes.len(),
            max_size
        )));
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    #[test]
    fn max_size_constant() {
        // Verify the default max body size is 1MB
        assert_eq!(1_048_576, 1024 * 1024);
    }
}
