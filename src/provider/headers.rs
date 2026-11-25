use http::{HeaderMap, HeaderName, HeaderValue};

/// Keep end-to-end headers, including repeated and non-UTF-8 values.
pub(crate) fn forwardable_headers(incoming: &HeaderMap) -> HeaderMap {
    let mut headers = incoming.clone();
    // Connection can nominate additional hop-by-hop headers, in multiple fields.
    for value in incoming.get_all("connection") {
        for token in value.as_bytes().split(|byte| *byte == b',') {
            if let Ok(name) = HeaderName::from_bytes(token.trim_ascii()) {
                headers.remove(name);
            }
        }
    }
    for name in [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "proxy-connection",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
        // The destination, serialized body and gateway credentials change.
        "host",
        "content-length",
        "content-encoding",
        "authorization",
        "x-api-key",
        "api-key",
        "cookie",
    ] {
        headers.remove(name);
    }
    headers
}

/// Client values win over provider defaults; upstream credentials stay local.
pub(crate) fn merge_request_headers(incoming: &HeaderMap, defaults: HeaderMap) -> HeaderMap {
    let mut headers = forwardable_headers(incoming);
    for (name, value) in &defaults {
        if matches!(name.as_str(), "authorization" | "x-api-key" | "api-key")
            || !headers.contains_key(name)
        {
            headers.insert(name.clone(), value.clone());
        }
    }
    headers
}

pub(crate) fn ensure_codex_identity(headers: &mut HeaderMap) {
    // Match the bundled CLIProxyAPI Codex defaults only when the client and
    // provider configuration supplied no identity. Never replace a real client.
    headers
        .entry("user-agent")
        .or_insert(HeaderValue::from_static(
            "codex-tui/0.135.0 (Mac OS 26.5.0; arm64) iTerm.app/3.6.10 (codex-tui; 0.135.0)",
        ));
    headers
        .entry("originator")
        .or_insert(HeaderValue::from_static("codex-tui"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_repeated_and_opaque_client_headers() {
        let mut incoming = HeaderMap::new();
        incoming.append("x-client-feature", HeaderValue::from_static("one"));
        incoming.append("x-client-feature", HeaderValue::from_static("two"));
        incoming.insert("x-opaque", HeaderValue::from_bytes(b"\xff").unwrap());
        assert_eq!(merge_request_headers(&incoming, HeaderMap::new()), incoming);
    }

    #[test]
    fn removes_transport_headers_and_connection_nominated_fields() {
        let mut incoming = HeaderMap::new();
        for (name, value) in [
            ("connection", "keep-alive, X-Internal"),
            ("x-internal", "private"),
            ("host", "gateway.local"),
            ("content-length", "999"),
            ("content-encoding", "gzip"),
            ("transfer-encoding", "chunked"),
            ("authorization", "Bearer gateway-secret"),
            ("x-api-key", "gateway-secret"),
            ("cookie", "gateway-session=secret"),
        ] {
            incoming.append(
                HeaderName::from_static(name),
                HeaderValue::from_static(value),
            );
        }
        incoming.append("connection", HeaderValue::from_static("X-Other"));
        incoming.insert("x-other", HeaderValue::from_static("private"));
        assert!(forwardable_headers(&incoming).is_empty());
    }

    #[test]
    fn client_identity_wins_and_provider_credentials_replace_gateway_credentials() {
        let incoming = HeaderMap::from_iter([
            (
                http::header::USER_AGENT,
                HeaderValue::from_static("real-client/1"),
            ),
            (
                http::header::AUTHORIZATION,
                HeaderValue::from_static("Bearer gateway"),
            ),
        ]);
        let defaults = HeaderMap::from_iter([
            (
                http::header::USER_AGENT,
                HeaderValue::from_static("configured-client/2"),
            ),
            (
                http::header::AUTHORIZATION,
                HeaderValue::from_static("Bearer upstream"),
            ),
        ]);
        let headers = merge_request_headers(&incoming, defaults);
        assert_eq!(headers["user-agent"], "real-client/1");
        assert_eq!(headers["authorization"], "Bearer upstream");
    }

    #[test]
    fn missing_codex_identity_uses_cliproxyapi_defaults() {
        let mut headers = HeaderMap::new();
        ensure_codex_identity(&mut headers);
        assert_eq!(
            headers["user-agent"],
            "codex-tui/0.135.0 (Mac OS 26.5.0; arm64) iTerm.app/3.6.10 (codex-tui; 0.135.0)"
        );
        assert_eq!(headers["originator"], "codex-tui");
    }

    #[test]
    fn codex_defaults_only_fill_missing_identity() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", HeaderValue::from_static("official-client/3"));
        ensure_codex_identity(&mut headers);
        assert_eq!(headers["user-agent"], "official-client/3");
        assert_eq!(headers["originator"], "codex-tui");
        headers.insert("originator", HeaderValue::from_static("codex_cli_rs"));
        ensure_codex_identity(&mut headers);
        assert_eq!(headers["originator"], "codex_cli_rs");
    }
}
