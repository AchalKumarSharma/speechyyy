use sha2::{Digest, Sha256};
use speechyyy::update::{
    download_and_verify_release, is_valid_tag, parse_sha256sums, release_archive_dir,
    release_archive_name, release_download_url, release_sha256sums_url, MAX_SHA256SUMS_BYTES,
    MAX_UPDATE_ARCHIVE_BYTES,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let count = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "speechyyy_test_{}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            count
        ));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct MockServer {
    url: String,
    shutdown_tx: Option<std::sync::mpsc::Sender<()>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockServer {
    fn spawn<F>(handler: F) -> Self
    where
        F: Fn(usize, &str) -> (u16, Vec<(String, String)>, Vec<u8>) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().expect("local addr");
        let url = format!("http://{}", addr);
        let (tx, rx) = std::sync::mpsc::channel();

        listener.set_nonblocking(true).unwrap();

        let handle = thread::spawn(move || {
            let mut req_idx = 0;
            loop {
                if rx.try_recv().is_ok() {
                    break;
                }
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buf = [0u8; 2048];
                        let n = stream.read(&mut buf).unwrap_or(0);
                        let req_str = String::from_utf8_lossy(&buf[..n]);
                        let (status, headers, body) = handler(req_idx, &req_str);
                        req_idx += 1;

                        let status_text = match status {
                            200 => "OK",
                            302 => "Found",
                            404 => "Not Found",
                            500 => "Internal Server Error",
                            _ => "Response",
                        };

                        let mut header_str = format!("HTTP/1.1 {} {}\r\n", status, status_text);
                        let mut has_cl = false;
                        for (k, v) in headers {
                            if k.eq_ignore_ascii_case("content-length") {
                                has_cl = true;
                            }
                            header_str.push_str(&format!("{}: {}\r\n", k, v));
                        }
                        if !has_cl {
                            header_str.push_str(&format!("Content-Length: {}\r\n", body.len()));
                        }
                        header_str.push_str("Connection: close\r\n\r\n");

                        let _ = stream.write_all(header_str.as_bytes());
                        let _ = stream.write_all(&body);
                        let _ = stream.flush();
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        MockServer {
            url,
            shutdown_tx: Some(tx),
            handle: Some(handle),
        }
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn compute_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[test]
fn test_parse_sha256sums_valid_entries() {
    let content = "\
# Checksums for speechyyy v0.2.0
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  speechyyy-v0.2.0-x86_64-unknown-linux-gnu.tar.gz
1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef *speechyyy-v0.2.0-aarch64-unknown-linux-gnu.tar.gz
";
    let hash = parse_sha256sums(content, "speechyyy-v0.2.0-x86_64-unknown-linux-gnu.tar.gz").unwrap();
    assert_eq!(hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");

    let hash_bin = parse_sha256sums(content, "speechyyy-v0.2.0-aarch64-unknown-linux-gnu.tar.gz").unwrap();
    assert_eq!(hash_bin, "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef");
}

#[test]
fn test_parse_sha256sums_duplicate_aborts() {
    let content = "\
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  speechyyy-v0.2.0.tar.gz
1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef  speechyyy-v0.2.0.tar.gz
";
    let err = parse_sha256sums(content, "speechyyy-v0.2.0.tar.gz").unwrap_err();
    assert!(err.contains("Duplicate entry for 'speechyyy-v0.2.0.tar.gz'"), "got: {}", err);
}

#[test]
fn test_parse_sha256sums_malformed_hex_aborts() {
    let content = "not_a_valid_hex_hash_too_short  speechyyy-v0.2.0.tar.gz\n";
    let err = parse_sha256sums(content, "speechyyy-v0.2.0.tar.gz").unwrap_err();
    assert!(err.contains("invalid SHA-256 hash"), "got: {}", err);
}

#[test]
fn test_parse_sha256sums_target_not_found_aborts() {
    let content = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  other-archive.tar.gz\n";
    let err = parse_sha256sums(content, "speechyyy-v0.2.0.tar.gz").unwrap_err();
    assert!(err.contains("Target file 'speechyyy-v0.2.0.tar.gz' not found"), "got: {}", err);
}

#[test]
fn test_download_and_verify_release_valid() {
    let dummy_archive = b"dummy tar gz payload for speechyyy test";
    let archive_hash = compute_sha256(dummy_archive);
    let target_filename = "speechyyy-v0.2.0-x86_64-unknown-linux-gnu.tar.gz";
    let sha_content = format!("{}  {}\n", archive_hash, target_filename);

    let sha_content_clone = sha_content.clone();
    let dummy_archive_vec = dummy_archive.to_vec();

    let server = MockServer::spawn(move |_idx, req| {
        if req.starts_with("GET /SHA256SUMS") {
            (200, vec![("Content-Type".into(), "text/plain".into())], sha_content_clone.as_bytes().to_vec())
        } else if req.starts_with("GET /archive.tar.gz") {
            (200, vec![("Content-Type".into(), "application/gzip".into())], dummy_archive_vec.clone())
        } else {
            (404, vec![], b"Not found".to_vec())
        }
    });

    let temp_dir = TempDir::new();
    let staged_archive = temp_dir.path().join("archive.tar.gz");
    let staged_sha = temp_dir.path().join("SHA256SUMS");

    let sha_url = format!("{}/SHA256SUMS", server.url);
    let archive_url = format!("{}/archive.tar.gz", server.url);

    let allow_loopback = |_url: &str| -> Result<(), String> { Ok(()) };

    let result = download_and_verify_release(
        &archive_url,
        &sha_url,
        target_filename,
        &staged_archive,
        &staged_sha,
        allow_loopback,
    );

    assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    assert_eq!(result.unwrap(), archive_hash);
    assert!(staged_archive.exists(), "Staged archive must be retained after verification");
    assert!(!staged_sha.exists(), "Staged SHA256SUMS should be cleaned up after verification");
}

#[test]
fn test_download_and_verify_release_hash_mismatch_aborts() {
    let dummy_archive = b"payload from server";
    let bogus_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let target_filename = "speechyyy-v0.2.0-x86_64-unknown-linux-gnu.tar.gz";
    let sha_content = format!("{}  {}\n", bogus_hash, target_filename);

    let sha_content_clone = sha_content.clone();
    let dummy_archive_vec = dummy_archive.to_vec();

    let server = MockServer::spawn(move |_idx, req| {
        if req.starts_with("GET /SHA256SUMS") {
            (200, vec![("Content-Type".into(), "text/plain".into())], sha_content_clone.as_bytes().to_vec())
        } else if req.starts_with("GET /archive.tar.gz") {
            (200, vec![("Content-Type".into(), "application/gzip".into())], dummy_archive_vec.clone())
        } else {
            (404, vec![], b"Not found".to_vec())
        }
    });

    let temp_dir = TempDir::new();
    let staged_archive = temp_dir.path().join("archive.tar.gz");
    let staged_sha = temp_dir.path().join("SHA256SUMS");

    let sha_url = format!("{}/SHA256SUMS", server.url);
    let archive_url = format!("{}/archive.tar.gz", server.url);

    let allow_loopback = |_url: &str| -> Result<(), String> { Ok(()) };

    let result = download_and_verify_release(
        &archive_url,
        &sha_url,
        target_filename,
        &staged_archive,
        &staged_sha,
        allow_loopback,
    );

    assert!(result.is_err(), "Expected mismatch error");
    let err = result.unwrap_err();
    assert!(err.contains("SHA-256 verification failed"), "got: {}", err);
    assert!(!staged_archive.exists(), "Staged archive must be removed on hash mismatch");
    assert!(!staged_sha.exists(), "Staged SHA must be cleaned up");
}

#[test]
fn test_download_and_verify_missing_sha256sums_404_aborts() {
    let server = MockServer::spawn(move |_idx, _req| {
        (404, vec![], b"Not Found".to_vec())
    });

    let temp_dir = TempDir::new();
    let staged_archive = temp_dir.path().join("archive.tar.gz");
    let staged_sha = temp_dir.path().join("SHA256SUMS");

    let sha_url = format!("{}/SHA256SUMS", server.url);
    let archive_url = format!("{}/archive.tar.gz", server.url);

    let allow_loopback = |_url: &str| -> Result<(), String> { Ok(()) };

    let result = download_and_verify_release(
        &archive_url,
        &sha_url,
        "speechyyy.tar.gz",
        &staged_archive,
        &staged_sha,
        allow_loopback,
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("Failed to download SHA256SUMS"), "got: {}", err);
    assert!(!staged_archive.exists());
    assert!(!staged_sha.exists());
}

#[test]
fn test_download_and_verify_disallowed_redirect_host_aborts() {
    let dummy_archive = b"secret payload";
    let archive_hash = compute_sha256(dummy_archive);
    let target_filename = "speechyyy.tar.gz";
    let sha_content = format!("{}  {}\n", archive_hash, target_filename);

    let sha_clone = sha_content.clone();
    let server = MockServer::spawn(move |_idx, req| {
        if req.starts_with("GET /SHA256SUMS") {
            (200, vec![("Content-Type".into(), "text/plain".into())], sha_clone.as_bytes().to_vec())
        } else if req.starts_with("GET /archive.tar.gz") {
            // Redirects to an untrusted host
            (302, vec![("Location".into(), "https://untrusted-malicious-site.com/payload.tar.gz".into())], b"".to_vec())
        } else {
            (404, vec![], b"".to_vec())
        }
    });

    let temp_dir = TempDir::new();
    let staged_archive = temp_dir.path().join("archive.tar.gz");
    let staged_sha = temp_dir.path().join("SHA256SUMS");

    let sha_url = format!("{}/SHA256SUMS", server.url);
    let archive_url = format!("{}/archive.tar.gz", server.url);

    // Policy only allows loopback URLs
    let strict_policy = |url: &str| -> Result<(), String> {
        if url.starts_with("http://127.0.0.1") {
            Ok(())
        } else {
            Err(format!("Host in '{}' is not allowed", url))
        }
    };

    let result = download_and_verify_release(
        &archive_url,
        &sha_url,
        target_filename,
        &staged_archive,
        &staged_sha,
        strict_policy,
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("not allowed"), "got: {}", err);
    assert!(!staged_archive.exists());
    assert!(!staged_sha.exists());
}

#[test]
fn test_size_caps_constants_and_enforcement() {
    assert_eq!(MAX_SHA256SUMS_BYTES, 256 * 1024);
    assert_eq!(MAX_UPDATE_ARCHIVE_BYTES, 150 * 1024 * 1024);

    // Verify fetch_with_policy enforces max_bytes cap
    let fetcher = speechyyy::model::UreqFetcher;
    let server = MockServer::spawn(|_idx, _req| {
        (200, vec![], vec![b'A'; 500])
    });

    let temp_dir = TempDir::new();
    let dest = temp_dir.path().join("dest.bin");
    let url = format!("{}/large", server.url);

    let res = fetcher.fetch_with_policy(
        &url,
        &dest,
        200, // max 200 bytes, but server sends 500
        None,
        &|_| Ok(()),
    );

    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.contains("exceeding allowed maximum"), "got: {}", err);
    assert!(!dest.exists(), "Oversized download must be removed");
}

#[test]
fn test_is_valid_tag_matrix() {
    // Valid tags
    assert!(is_valid_tag("v0.2.0"));
    assert!(is_valid_tag("v0.2.0-rc1"));
    assert!(is_valid_tag("v1.10.3-beta.2"));
    assert!(is_valid_tag("v0.0.1"));
    assert!(is_valid_tag("v10.200.3000-alpha.1.2"));

    // Invalid tags
    assert!(!is_valid_tag("0.2.0"), "Missing leading 'v'");
    assert!(!is_valid_tag("latest"), "Word 'latest' is not semver tag");
    assert!(!is_valid_tag("v0.2"), "Missing patch version");
    assert!(!is_valid_tag("v0.2.0/../x"), "Path traversal prohibited");
    assert!(!is_valid_tag("v0.2.0?x=1"), "Query string prohibited");
    assert!(!is_valid_tag("v0.2.0 rc1"), "Whitespace prohibited");
    assert!(!is_valid_tag(" v0.2.0"), "Leading whitespace prohibited");
    assert!(!is_valid_tag("v0.2.0 "), "Trailing whitespace prohibited");
    assert!(!is_valid_tag(""), "Empty string prohibited");
    assert!(!is_valid_tag("v0.2.0-"), "Empty pre-release suffix prohibited");
    assert!(!is_valid_tag("v0.2.0-rc1/2"), "Slash in pre-release prohibited");
}

#[test]
fn test_release_asset_naming_scheme() {
    assert_eq!(
        release_archive_name("v0.2.0", "x86_64"),
        "speechyyy-v0.2.0-x86_64-linux.tar.gz"
    );
    assert_eq!(
        release_archive_name("v0.2.0-rc1", "aarch64"),
        "speechyyy-v0.2.0-rc1-aarch64-linux.tar.gz"
    );
    assert_eq!(
        release_archive_dir("v0.2.0", "x86_64"),
        "speechyyy-v0.2.0-x86_64-linux"
    );
    assert_eq!(
        release_download_url("v0.2.0", "x86_64"),
        "https://github.com/AchalKumarSharma/speechyyy/releases/download/v0.2.0/speechyyy-v0.2.0-x86_64-linux.tar.gz"
    );
    assert_eq!(
        release_sha256sums_url("v0.2.0"),
        "https://github.com/AchalKumarSharma/speechyyy/releases/download/v0.2.0/SHA256SUMS"
    );
}

#[test]
fn test_handle_update_command_rejects_invalid_tag() {
    let res = speechyyy::update::handle_update_command(false, Some("0.2.0"));
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.contains("Invalid release tag"), "Got: {}", err);

    let res_traversal = speechyyy::update::handle_update_command(false, Some("v0.2.0/../hack"));
    assert!(res_traversal.is_err());
    let err_traversal = res_traversal.unwrap_err();
    assert!(err_traversal.contains("Invalid release tag"), "Got: {}", err_traversal);
}
