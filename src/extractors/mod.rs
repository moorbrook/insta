pub mod archive;
pub mod github;
pub mod instapaper;
pub mod readability;
pub mod youtube;

use anyhow::Context;
use encoding_rs::{CoderResult, Decoder, Encoding, UTF_8};

pub struct ExtractedArticle {
    pub title: String,
    pub content: String,
}

/// Match an exact domain or one of its subdomains.
///
/// Parsing the host avoids treating a domain mentioned in a path, query,
/// fragment, or username as the destination host.
pub(crate) fn host_is_domain_or_subdomain(url: &str, domain: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };

    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

/// Maximum decompressed response-body and decoded-text size (8 MiB each).
///
/// At the default 20-worker concurrency this caps retained decoded text at
/// roughly 160 MiB before parser-specific overhead.
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

const DECODE_BUFFER_BYTES: usize = 8 * 1024;

struct BoundedText {
    decoder: Decoder,
    input_bytes: usize,
    output: Vec<u8>,
    limit: usize,
}

impl BoundedText {
    fn new(encoding: &'static Encoding, limit: usize) -> Self {
        Self {
            decoder: encoding.new_decoder(),
            input_bytes: 0,
            output: Vec::new(),
            limit,
        }
    }

    fn push(&mut self, chunk: &[u8]) -> anyhow::Result<()> {
        let input_bytes = self
            .input_bytes
            .checked_add(chunk.len())
            .context("response body size overflow")?;
        if input_bytes > self.limit {
            anyhow::bail!(
                "decompressed response body exceeds {}-byte limit",
                self.limit
            );
        }
        self.input_bytes = input_bytes;
        self.decode(chunk, false)
    }

    fn finish(mut self) -> anyhow::Result<String> {
        self.decode(&[], true)?;
        String::from_utf8(self.output).context("text decoder produced invalid UTF-8")
    }

    fn decode(&mut self, source: &[u8], last: bool) -> anyhow::Result<()> {
        let mut read = 0;

        loop {
            let mut decoded = [0; DECODE_BUFFER_BYTES];
            let (result, consumed, written, _) =
                self.decoder
                    .decode_to_utf8(&source[read..], &mut decoded, last);
            read = read
                .checked_add(consumed)
                .context("text decoder input offset overflow")?;

            let output_bytes = self
                .output
                .len()
                .checked_add(written)
                .context("decoded text size overflow")?;
            if output_bytes > self.limit {
                anyhow::bail!("decoded text exceeds {}-byte limit", self.limit);
            }
            self.output
                .try_reserve(written)
                .context("failed to reserve decoded text buffer")?;
            self.output.extend_from_slice(&decoded[..written]);

            match result {
                CoderResult::InputEmpty => return Ok(()),
                CoderResult::OutputFull if consumed != 0 || written != 0 => {}
                CoderResult::OutputFull => {
                    anyhow::bail!("text decoder made no progress");
                }
            }
        }
    }
}

fn response_encoding(headers: &reqwest::header::HeaderMap) -> &'static Encoding {
    headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(declared_charset)
        .and_then(|charset| Encoding::for_label(charset.as_bytes()))
        .unwrap_or(UTF_8)
}

fn declared_charset(content_type: &str) -> Option<&str> {
    content_type.split(';').skip(1).find_map(|parameter| {
        let (name, value) = parameter.split_once('=')?;
        if !name.trim().eq_ignore_ascii_case("charset") {
            return None;
        }

        let value = value.trim();
        Some(
            value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .unwrap_or(value),
        )
    })
}

/// Read a response body as text with a size limit.
///
/// The limit is enforced on chunks after reqwest's automatic decompression and
/// again on the UTF-8 output after charset decoding. Crossing either limit
/// rejects the response without reading or decoding the remainder.
pub async fn read_body(response: reqwest::Response) -> anyhow::Result<String> {
    read_body_with_limit(response, MAX_BODY_BYTES).await
}

async fn read_body_with_limit(
    mut response: reqwest::Response,
    limit: usize,
) -> anyhow::Result<String> {
    if let Some(len) = response.content_length() {
        if len > u64::try_from(limit).unwrap_or(u64::MAX) {
            anyhow::bail!("response body is {len} bytes (limit: {limit} bytes)");
        }
    }

    let mut text = BoundedText::new(response_encoding(response.headers()), limit);
    while let Some(chunk) = response.chunk().await? {
        text.push(&chunk)?;
    }
    text.finish()
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use encoding_rs::{UTF_8, WINDOWS_1252};

    use super::{host_is_domain_or_subdomain, read_body_with_limit, BoundedText};

    #[test]
    fn host_matching_accepts_exact_domains_and_subdomains() {
        assert!(host_is_domain_or_subdomain(
            "https://youtube.com/watch?v=1",
            "youtube.com"
        ));
        assert!(host_is_domain_or_subdomain(
            "https://www.youtube.com/watch?v=1",
            "youtube.com"
        ));
        assert!(host_is_domain_or_subdomain(
            "HTTPS://WWW.YOUTUBE.COM/watch?v=1",
            "youtube.com"
        ));
    }

    #[test]
    fn host_matching_rejects_domain_text_outside_the_host() {
        for url in [
            "https://example.com/youtube.com/watch",
            "https://example.com/?next=https://youtube.com",
            "https://youtube.com@example.com/watch",
            "https://notyoutube.com/watch",
            "https://youtube.com.evil.example/watch",
            "not a url containing youtube.com",
        ] {
            assert!(
                !host_is_domain_or_subdomain(url, "youtube.com"),
                "misclassified {url}"
            );
        }
    }

    fn decode_segments(
        encoding: &'static encoding_rs::Encoding,
        segments: &[&[u8]],
        limit: usize,
    ) -> anyhow::Result<String> {
        let mut text = BoundedText::new(encoding, limit);
        for segment in segments {
            text.push(segment)?;
        }
        text.finish()
    }

    fn serve_once(response: Vec<u8>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("read test server address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept test request");
            let mut request = [0; 1024];
            let _ = stream.read(&mut request);
            stream.write_all(&response).expect("write test response");
        });
        (format!("http://{address}/"), server)
    }

    fn chunked_response(content_type: &str, chunks: &[&[u8]]) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n\
             Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
        )
        .into_bytes();
        for chunk in chunks {
            response.extend_from_slice(format!("{:X}\r\n", chunk.len()).as_bytes());
            response.extend_from_slice(chunk);
            response.extend_from_slice(b"\r\n");
        }
        response.extend_from_slice(b"0\r\n\r\n");
        response
    }

    async fn fetch_raw(response: Vec<u8>) -> reqwest::Response {
        let (url, server) = serve_once(response);
        let response = reqwest::get(url).await.expect("fetch test response");
        server.join().expect("join test server");
        response
    }

    #[test]
    fn chunk_partitioning_does_not_change_decoding() {
        let utf8 = "start € end".as_bytes();
        let expected = decode_segments(UTF_8, &[utf8], utf8.len()).expect("decode whole body");

        for split in 0..=utf8.len() {
            let actual = decode_segments(UTF_8, &[&utf8[..split], &utf8[split..]], utf8.len())
                .expect("decode partitioned body");
            assert_eq!(actual, expected, "changed at split {split}");
        }
    }

    #[test]
    fn exact_limits_pass_and_adjacent_limits_fail() {
        let input = b"12345678";
        assert_eq!(
            decode_segments(UTF_8, &[input], input.len()).expect("exact limit"),
            "12345678"
        );
        assert!(decode_segments(UTF_8, &[input], input.len() - 1).is_err());

        let windows_1252 = b"\x93quoted\x94";
        assert_eq!(
            decode_segments(WINDOWS_1252, &[windows_1252], 12).expect("decoded exact limit"),
            "\u{201c}quoted\u{201d}"
        );
        assert!(decode_segments(WINDOWS_1252, &[windows_1252], 11).is_err());
    }

    #[tokio::test]
    async fn chunked_body_over_limit_is_rejected() {
        let response = fetch_raw(chunked_response(
            "text/plain; charset=utf-8",
            &[b"1234", b"5678", b"9"],
        ))
        .await;

        let error = read_body_with_limit(response, 8)
            .await
            .expect_err("body exceeds limit");

        assert!(error
            .to_string()
            .contains("decompressed response body exceeds 8-byte limit"));
    }

    #[tokio::test]
    async fn charset_decoding_handles_multibyte_transport_chunks() {
        let response = fetch_raw(chunked_response(
            "text/plain; charset=windows-1252",
            &[b"\x93hello", b"\x94"],
        ))
        .await;

        assert_eq!(
            read_body_with_limit(response, 64)
                .await
                .expect("decode body"),
            "\u{201c}hello\u{201d}"
        );
    }

    #[tokio::test]
    async fn known_content_length_over_limit_is_rejected_before_body_read() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\
                    Content-Length: 9\r\nConnection: close\r\n\r\n123456789"
            .to_vec();
        let response = fetch_raw(raw).await;

        let error = read_body_with_limit(response, 8)
            .await
            .expect_err("known body length exceeds limit");

        assert!(error
            .to_string()
            .contains("response body is 9 bytes (limit: 8 bytes)"));
    }

    #[tokio::test]
    async fn gzip_expansion_is_limited_after_decompression() {
        const GZIP_4096_AS: &[u8] = &[
            31, 139, 8, 0, 0, 0, 0, 0, 0, 3, 237, 193, 1, 13, 0, 0, 0, 194, 160, 108, 239, 95, 202,
            30, 14, 40, 0, 0, 0, 224, 221, 0, 64, 52, 166, 254, 0, 16, 0, 0,
        ];
        let mut raw = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n\
             Content-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            GZIP_4096_AS.len()
        )
        .into_bytes();
        raw.extend_from_slice(GZIP_4096_AS);
        let response = fetch_raw(raw).await;

        assert_eq!(
            response.content_length(),
            None,
            "reqwest must hide the compressed length"
        );
        let error = read_body_with_limit(response, 128)
            .await
            .expect_err("expanded body exceeds limit");

        assert!(error
            .to_string()
            .contains("decompressed response body exceeds 128-byte limit"));
    }
}
