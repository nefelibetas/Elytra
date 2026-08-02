use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;

use super::extract::native_classifier_matches;
use super::hash::file_sha1_matches;
use crate::config::downloader::model::DownloadSource;
use crate::downloader::fetcher::{self, http_libs};
use crate::downloader::model::{Library, OsRule, Rule};

fn matches_rules(rules: &[Rule]) -> bool {
    let mut allowed = false;
    for rule in rules {
        let os_match = match &rule.os {
            Some(OsRule { name: Some(n) }) => n == "windows",
            _ => true,
        };
        match rule.action.as_str() {
            "allow" if os_match => allowed = true,
            "disallow" if os_match => return false,
            _ => {}
        }
    }
    allowed
}

fn should_download(lib: &Library) -> bool {
    if let Some(rules) = &lib.rules {
        if !matches_rules(rules) {
            return false;
        }
    }
    if !native_classifier_matches(&lib.name) {
        return false;
    }
    true
}

/// Count how many libraries still need downloading (filtering + existence + sha1 check).
pub fn count_pending(libraries: &[Library], lib_dir: &Path) -> usize {
    libraries.iter().filter(|lib| {
        if !should_download(lib) { return false; }
        let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) else { return false; };
        let dest = lib_dir.join(&artifact.path);
        if dest.exists() && file_sha1_matches(&dest, &artifact.sha1) { return false; }
        true
    }).count()
}

/// Count libraries that need to be present (passes filter + has artifact), regardless of local state.
pub fn count_total(libraries: &[Library]) -> usize {
    libraries.iter().filter(|lib| {
        if !should_download(lib) { return false; }
        lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()).is_some()
    }).count()
}

pub async fn download_libraries(
    libraries: &[Library],
    libraries_dir: &Path,
    concurrency: usize,
    source: &DownloadSource,
    counter: Option<Arc<AtomicU32>>,
    bytes_counter: Option<Arc<AtomicU64>>,
) -> Result<(), Vec<String>> {
    let mut tasks: Vec<(String, std::path::PathBuf)> = Vec::new();

    for lib in libraries {
        if !should_download(lib) {
            continue;
        }
        let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) else {
            continue;
        };
        let dest = libraries_dir.join(&artifact.path);
        if dest.exists() && file_sha1_matches(&dest, &artifact.sha1) {
            continue;
        }
        let url = source.mirror_url(&artifact.url);
        tasks.push((url, dest));
    }

    if tasks.is_empty() {
        return Ok(());
    }

    let sem = Arc::new(Semaphore::new(concurrency));
    let mut errors = Vec::new();
    let mut handles = Vec::new();

    for (url, dest) in tasks {
        let permit = sem.clone();
        let ctr = counter.clone();
        let bc = bytes_counter.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit.acquire_owned().await.unwrap();
            match fetcher::fetch_with(http_libs(), &url, &dest, None::<fn(u64, u64)>).await {
                Ok(dl) => {
                    if let Some(ref c) = ctr { c.fetch_add(1, Ordering::Relaxed); }
                    if let Some(ref b) = bc { b.fetch_add(dl.bytes, Ordering::SeqCst); }
                    Ok(())
                }
                Err(e) => Err(format!("{}: {}", dest.display(), e)),
            }
        }));
    }

    for handle in handles {
        if let Err(e) = handle.await.unwrap() {
            errors.push(e);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
