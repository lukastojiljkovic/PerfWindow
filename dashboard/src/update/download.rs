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
    #[error("cancelled")]
    Cancelled,
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
pub fn start_download(
    url: String,
    dest: PathBuf,
    expected_size: u64,
    progress: SharedProgress,
    cancelled: Arc<AtomicBool>,
    repaint: impl Fn() + Send + 'static,
    on_finish: impl FnOnce(Result<PathBuf, DownloadError>) + Send + 'static,
) {
    std::thread::spawn(move || {
        let result = run_download(&url, &dest, expected_size, &progress, &cancelled, &repaint);
        on_finish(result);
    });
}

fn run_download(
    url: &str,
    dest: &Path,
    expected_size: u64,
    progress: &SharedProgress,
    cancelled: &AtomicBool,
    repaint: &(impl Fn() + ?Sized),
) -> Result<PathBuf, DownloadError> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .user_agent(&format!("PerfWindow/{}", env!("CARGO_PKG_VERSION")))
        .build();

    let response = agent.get(url).call().map_err(|e| match e {
        ureq::Error::Status(code, _) => DownloadError::HttpStatus(code),
        ureq::Error::Transport(t) => DownloadError::Network(t.to_string()),
    })?;

    let mut reader = response.into_reader();
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
        )
    };
    finalize(dest, copied, expected_size)
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
}
