//! Streaming installer download with progress and cancellation.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// Tunable chunk size: a compromise between progress-update cadence and
/// per-write overhead.
const CHUNK_SIZE: usize = 64 * 1024;

/// Snapshot of an in-flight download.
#[derive(Debug, Clone, Default)]
pub struct DownloadProgress {
    pub bytes: u64,
    pub total: u64,
}

impl DownloadProgress {
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.bytes as f64 / self.total as f64) as f32
        }
    }
}

pub type SharedProgress = Arc<Mutex<DownloadProgress>>;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("network error: {0}")]
    Network(String),
    #[error("HTTP status {0}")]
    HttpStatus(u16),
    #[error("disk write failed: {0}")]
    Io(String),
    #[error("download size did not match the asset metadata")]
    SizeMismatch,
    #[error("this release has no checksum file, so the installer was not downloaded")]
    MissingChecksum,
    #[error("the checksum file for this release is malformed")]
    ChecksumMalformed,
    #[error("the downloaded installer did not match its checksum")]
    ChecksumMismatch,
    #[error("the release points outside this project's GitHub download URLs")]
    UnusableUrl,
    #[error("cancelled")]
    Cancelled,
}

/// One installer download: the asset URL, its checksum sidecar, the size the
/// release advertises and the file the bytes are written to.
pub struct DownloadRequest {
    pub url: String,
    /// `None` when the release has no `.sha256` asset; the download is then
    /// refused rather than trusted.
    pub sidecar_url: Option<String>,
    /// Must be unique per attempt: two workers writing one path corrupt each
    /// other's output.
    pub dest: PathBuf,
    pub expected_size: u64,
}

/// Begin a download on a background thread; returns immediately. `url` is
/// the asset download URL from the `Release`. `expected_size` is the
/// `assets[].size` from the release JSON; the download fails if the actual
/// bytes do not match. `dest` is the absolute path the bytes are written to;
/// it must be unique per attempt — two workers writing one path can corrupt
/// each other's output.
///
/// `repaint` is invoked after every progress update; in production it wraps
/// [`egui::Context::request_repaint`]. `on_finish` is invoked once with the
/// result; it runs on the download thread, so it must not touch UI state
/// directly — it should write into a shared mutex that the UI thread polls.
///
/// On any failure — cancellation, network/disk error, size mismatch — the
/// partial file at `dest` is removed before `on_finish` runs.
///
/// The installer's `.sha256` sidecar is fetched first and the streaming bytes
/// are hashed on the way through; the file is deleted on a size or checksum
/// mismatch, so a failed download never survives on disk.
pub fn start_download(
    request: DownloadRequest,
    progress: SharedProgress,
    cancelled: Arc<AtomicBool>,
    repaint: impl Fn() + Send + 'static,
    on_finish: impl FnOnce(Result<(PathBuf, String), DownloadError>) + Send + 'static,
) {
    std::thread::spawn(move || {
        let result = if urls_are_acceptable(&request) {
            run_download(
                &request.url,
                request.sidecar_url.as_deref(),
                &request.dest,
                request.expected_size,
                &progress,
                &cancelled,
                &repaint,
            )
        } else {
            Err(DownloadError::UnusableUrl)
        };
        on_finish(result);
    });
}

/// The downloader only follows this project's own release URLs: `https` on
/// github.com under the repository's download path. Re-checked at the entry
/// point so a `Release` built by hand, or a cache file edited on disk, can
/// never point the transfer at another host.
fn urls_are_acceptable(request: &DownloadRequest) -> bool {
    crate::update::release::is_acceptable_asset_url(&request.url)
        && request
            .sidecar_url
            .as_deref()
            .is_none_or(crate::update::release::is_acceptable_asset_url)
}

fn run_download(
    url: &str,
    sidecar_url: Option<&str>,
    dest: &Path,
    expected_size: u64,
    progress: &SharedProgress,
    cancelled: &AtomicBool,
    repaint: &(impl Fn() + ?Sized),
) -> Result<(PathBuf, String), DownloadError> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .user_agent(&format!("PerfWindow/{}", env!("CARGO_PKG_VERSION")))
        .build();

    let expected_hash = fetch_expected_hash(&agent, sidecar_url)?;

    let response = agent.get(url).call().map_err(|e| match e {
        ureq::Error::Status(code, _) => DownloadError::HttpStatus(code),
        ureq::Error::Transport(t) => DownloadError::Network(t.to_string()),
    })?;

    let mut reader = response.into_reader();
    let mut hasher = ring::digest::Context::new(&ring::digest::SHA256);
    // Scoped so the file handle is closed before `finalize` may delete it.
    let copied = {
        let mut file = std::fs::File::create(dest).map_err(|e| DownloadError::Io(e.to_string()))?;
        copy_with_progress(
            &mut reader,
            &mut file,
            expected_size,
            progress,
            cancelled,
            repaint,
            Some(&mut hasher),
        )
    };
    let path = finalize(dest, copied, expected_size)?;

    let actual_hash = crate::update::install::hex_lower(hasher.finish().as_ref());
    if actual_hash != expected_hash {
        let _ = std::fs::remove_file(&path);
        return Err(DownloadError::ChecksumMismatch);
    }
    Ok((path, actual_hash))
}

/// Fetch the `.sha256` sidecar and parse it. A missing asset, a 404, or a body
/// that does not name the installer is refused before the installer download
/// starts.
fn fetch_expected_hash(
    agent: &ureq::Agent,
    sidecar_url: Option<&str>,
) -> Result<String, DownloadError> {
    let Some(url) = sidecar_url else {
        return Err(DownloadError::MissingChecksum);
    };
    let response = agent.get(url).call().map_err(|e| match e {
        ureq::Error::Status(404, _) => DownloadError::MissingChecksum,
        ureq::Error::Status(code, _) => DownloadError::HttpStatus(code),
        ureq::Error::Transport(t) => DownloadError::Network(t.to_string()),
    })?;
    let body = response
        .into_string()
        .map_err(|e| DownloadError::Network(e.to_string()))?;
    crate::update::release::parse_sidecar(&body).ok_or(DownloadError::ChecksumMalformed)
}

/// Turn the copy result into the download outcome. The byte count comes from
/// the worker's local counter, never from the shared progress — another
/// attempt's worker could have written into a shared counter and masked a
/// short or corrupt file. Every failure removes the partial file so no
/// truncated installer survives to a later attempt.
fn finalize(
    dest: &Path,
    copied: Result<u64, DownloadError>,
    expected_size: u64,
) -> Result<PathBuf, DownloadError> {
    match copied {
        Ok(bytes) if bytes == expected_size => Ok(dest.to_path_buf()),
        Ok(_) => {
            let _ = std::fs::remove_file(dest);
            Err(DownloadError::SizeMismatch)
        }
        Err(e) => {
            let _ = std::fs::remove_file(dest);
            Err(e)
        }
    }
}

/// Generic copy that drives the progress + cancellation contract. Extracted
/// so the unit tests can exercise it without an HTTP client. Returns the
/// number of bytes copied, counted locally.
fn copy_with_progress(
    reader: &mut dyn Read,
    writer: &mut dyn Write,
    total: u64,
    progress: &SharedProgress,
    cancelled: &AtomicBool,
    repaint: &(impl Fn() + ?Sized),
    mut hasher: Option<&mut ring::digest::Context>,
) -> Result<u64, DownloadError> {
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut bytes: u64 = 0;
    {
        let mut p = progress.lock().unwrap_or_else(PoisonError::into_inner);
        p.bytes = 0;
        p.total = total;
    }
    repaint();

    loop {
        if cancelled.load(Ordering::SeqCst) {
            return Err(DownloadError::Cancelled);
        }
        let n = reader
            .read(&mut buf)
            .map_err(|e| DownloadError::Network(e.to_string()))?;
        if n == 0 {
            break;
        }
        writer
            .write_all(&buf[..n])
            .map_err(|e| DownloadError::Io(e.to_string()))?;
        if let Some(context) = hasher.as_deref_mut() {
            context.update(&buf[..n]);
        }
        bytes += n as u64;
        progress
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .bytes = bytes;
        repaint();
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn copies_all_bytes_and_reports_progress() {
        let body = vec![0xABu8; 200_000];
        let total = body.len() as u64;
        let progress: SharedProgress = Arc::new(Mutex::new(DownloadProgress::default()));
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut sink: Vec<u8> = Vec::new();
        let result = copy_with_progress(
            &mut Cursor::new(body),
            &mut sink,
            total,
            &progress,
            &cancelled,
            &|| {},
            None,
        );
        assert_eq!(result.unwrap(), total);
        assert_eq!(sink.len() as u64, total);
        let snap = progress.lock().unwrap().clone();
        assert_eq!(snap.bytes, total);
        assert_eq!(snap.total, total);
    }

    #[test]
    fn cancellation_aborts_mid_stream() {
        let body = vec![0u8; 1_000_000];
        let total = body.len() as u64;
        let progress: SharedProgress = Arc::new(Mutex::new(DownloadProgress::default()));
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel_handle = Arc::clone(&cancelled);
        let mut sink: Vec<u8> = Vec::new();

        let result = copy_with_progress(
            &mut Cursor::new(body),
            &mut sink,
            total,
            &progress,
            &cancelled,
            &move || cancel_handle.store(true, Ordering::SeqCst),
            None,
        );
        assert!(matches!(result, Err(DownloadError::Cancelled)));
    }

    fn temp_file_with(name: &str, contents: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, contents).expect("test file writes");
        path
    }

    #[test]
    fn finalize_keeps_the_file_when_byte_count_matches() {
        let path = temp_file_with("pw-dl-finalize-ok.exe", &[1u8; 16]);
        let result = finalize(&path, Ok(16), 16);
        assert_eq!(result.unwrap(), path);
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn finalize_deletes_the_file_on_size_mismatch() {
        let path = temp_file_with("pw-dl-finalize-short.exe", &[1u8; 8]);
        let result = finalize(&path, Ok(8), 16);
        assert!(matches!(result, Err(DownloadError::SizeMismatch)));
        assert!(!path.exists());
    }

    #[test]
    fn finalize_deletes_the_file_on_cancel() {
        let path = temp_file_with("pw-dl-finalize-cancel.exe", &[1u8; 8]);
        let result = finalize(&path, Err(DownloadError::Cancelled), 16);
        assert!(matches!(result, Err(DownloadError::Cancelled)));
        assert!(!path.exists());
    }

    #[test]
    fn finalize_deletes_the_file_on_error() {
        let path = temp_file_with("pw-dl-finalize-err.exe", &[1u8; 8]);
        let result = finalize(&path, Err(DownloadError::Network("reset".into())), 16);
        assert!(matches!(result, Err(DownloadError::Network(_))));
        assert!(!path.exists());
    }

    #[test]
    fn finalize_trusts_the_local_counter_over_file_length() {
        // A full-length file with a zero-hole (another worker's resumed write
        // after this attempt's truncation) must not pass: the local counter
        // says fewer bytes than expected even though the file size matches.
        let path = temp_file_with("pw-dl-finalize-hole.exe", &[0u8; 16]);
        let result = finalize(&path, Ok(8), 16);
        assert!(matches!(result, Err(DownloadError::SizeMismatch)));
        assert!(!path.exists());
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        ring::digest::digest(&ring::digest::SHA256, bytes)
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// A tiny one-shot HTTP server for the download tests. Each accepted
    /// connection is answered with the body registered for its request path;
    /// the connection then closes. The thread keeps serving until the test
    /// stops it, so a client that opens more than one connection (a retry, a
    /// pooled connection) cannot deadlock the run.
    struct LocalServer {
        base: String,
        stop: Arc<AtomicBool>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl LocalServer {
        fn start(routes: Vec<(String, Vec<u8>)>) -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener binds");
            listener
                .set_nonblocking(true)
                .expect("listener goes non-blocking");
            let addr = listener.local_addr().expect("listener has an address");
            let stop = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&stop);
            let handle = std::thread::spawn(move || {
                let mut idle = 0;
                while !flag.load(Ordering::SeqCst) && idle < 500 {
                    match listener.accept() {
                        Ok((mut socket, _)) => {
                            idle = 0;
                            let mut buf = [0u8; 8192];
                            let read = socket.read(&mut buf).unwrap_or(0);
                            let request = String::from_utf8_lossy(&buf[..read]).to_string();
                            let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
                            let body = routes
                                .iter()
                                .find(|(route, _)| *route == path)
                                .map(|(_, body)| body.clone())
                                .unwrap_or_default();
                            let head = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let _ = socket.write_all(head.as_bytes());
                            let _ = socket.write_all(&body);
                            let _ = socket.shutdown(std::net::Shutdown::Both);
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            idle += 1;
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            });
            Self {
                base: format!("http://{addr}"),
                stop,
                handle: Some(handle),
            }
        }

        fn join(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    #[test]
    fn a_release_without_a_sidecar_is_refused_before_downloading() {
        let dest = std::env::temp_dir().join("pw-dl-missing-sidecar.exe");
        let _ = std::fs::remove_file(&dest);
        let progress: SharedProgress = Arc::new(Mutex::new(DownloadProgress::default()));
        let result = run_download(
            "http://127.0.0.1:1/PerfWindow-Setup.exe",
            None,
            &dest,
            16,
            &progress,
            &AtomicBool::new(false),
            &|| {},
        );
        assert!(matches!(result, Err(DownloadError::MissingChecksum)));
        assert!(!dest.exists(), "nothing may be written without a checksum");
    }

    #[test]
    fn the_download_entry_point_only_accepts_this_project_s_github_urls() {
        let request = |url: &str, sidecar: Option<&str>| DownloadRequest {
            url: url.to_owned(),
            sidecar_url: sidecar.map(str::to_owned),
            dest: std::env::temp_dir().join("pw-dl-policy.exe"),
            expected_size: 1,
        };
        let ours = "https://github.com/lukastojiljkovic/PerfWindow/releases/download/v1/PerfWindow-Setup.exe";
        assert!(urls_are_acceptable(&request(ours, None)));
        assert!(urls_are_acceptable(&request(
            ours,
            Some(&format!("{ours}.sha256"))
        )));
        assert!(!urls_are_acceptable(&request(
            "https://evil.example/PerfWindow-Setup.exe",
            None
        )));
        assert!(!urls_are_acceptable(&request(
            "http://127.0.0.1/PerfWindow-Setup.exe",
            None
        )));
        assert!(!urls_are_acceptable(&request(
            ours,
            Some("https://evil.example/PerfWindow-Setup.exe.sha256")
        )));
    }

    #[test]
    fn a_malformed_sidecar_is_refused() {
        let mut server = LocalServer::start(vec![(
            "/PerfWindow-Setup.exe.sha256".into(),
            b"not a checksum".to_vec(),
        )]);
        let dest = std::env::temp_dir().join("pw-dl-malformed-sidecar.exe");
        let _ = std::fs::remove_file(&dest);
        let progress: SharedProgress = Arc::new(Mutex::new(DownloadProgress::default()));
        let result = run_download(
            &format!("{}/PerfWindow-Setup.exe", server.base),
            Some(&format!("{}/PerfWindow-Setup.exe.sha256", server.base)),
            &dest,
            16,
            &progress,
            &AtomicBool::new(false),
            &|| {},
        );
        server.join();
        assert!(
            matches!(result, Err(DownloadError::ChecksumMalformed)),
            "expected a malformed-checksum refusal, got {result:?}"
        );
        assert!(!dest.exists());
    }

    #[test]
    fn a_checksummed_installer_downloads_and_verifies_end_to_end() {
        let body = b"installer bytes, not really an installer".to_vec();
        let hash = sha256_hex(&body);
        let sidecar = format!("{}  PerfWindow-Setup.exe\r\n", hash.to_uppercase());
        let mut server = LocalServer::start(vec![
            ("/PerfWindow-Setup.exe.sha256".into(), sidecar.into_bytes()),
            ("/PerfWindow-Setup.exe".into(), body.clone()),
        ]);
        let dest = std::env::temp_dir().join("pw-dl-e2e-good.exe");
        let _ = std::fs::remove_file(&dest);
        let progress: SharedProgress = Arc::new(Mutex::new(DownloadProgress::default()));

        let result = run_download(
            &format!("{}/PerfWindow-Setup.exe", server.base),
            Some(&format!("{}/PerfWindow-Setup.exe.sha256", server.base)),
            &dest,
            body.len() as u64,
            &progress,
            &AtomicBool::new(false),
            &|| {},
        );
        server.join();

        let (path, verified) = match result {
            Ok(value) => value,
            Err(e) => panic!("a matching checksum must accept the file, got {e:?}"),
        };
        assert_eq!(path, dest);
        assert_eq!(verified, hash);
        assert_eq!(std::fs::read(&dest).expect("file is on disk"), body);
        assert_eq!(progress.lock().unwrap().bytes, body.len() as u64);
        let _ = std::fs::remove_file(&dest);
    }

    #[test]
    fn a_corrupted_installer_is_rejected_and_deleted() {
        let body = b"installer bytes".to_vec();
        let expected = sha256_hex(b"different bytes");
        let sidecar = format!("{expected}  PerfWindow-Setup.exe");
        let mut server = LocalServer::start(vec![
            ("/PerfWindow-Setup.exe.sha256".into(), sidecar.into_bytes()),
            ("/PerfWindow-Setup.exe".into(), body.clone()),
        ]);
        let dest = std::env::temp_dir().join("pw-dl-e2e-corrupt.exe");
        let _ = std::fs::remove_file(&dest);
        let progress: SharedProgress = Arc::new(Mutex::new(DownloadProgress::default()));

        let result = run_download(
            &format!("{}/PerfWindow-Setup.exe", server.base),
            Some(&format!("{}/PerfWindow-Setup.exe.sha256", server.base)),
            &dest,
            body.len() as u64,
            &progress,
            &AtomicBool::new(false),
            &|| {},
        );
        server.join();

        assert!(
            matches!(result, Err(DownloadError::ChecksumMismatch)),
            "expected a mismatch, got {result:?}"
        );
        assert!(!dest.exists(), "a mismatched installer must be deleted");
    }
}
