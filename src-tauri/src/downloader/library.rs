use std::path::Path;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::downloader::fetcher;
use crate::downloader::model::{Library, OsRule, Rule};

fn matches_rules(rules: &[Rule]) -> bool {
    let mut allowed = false;
    for rule in rules {
        let os_match = match &rule.os {
            Some(OsRule { name: Some(n) }) => n == "windows",
            _ => true,
        };
        let this_match = os_match;
        match rule.action.as_str() {
            "allow" if this_match => allowed = true,
            "disallow" if this_match => return false,
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
    if lib.name.contains("-natives-") || lib.name.contains("natives-windows") {
        return false;
    }
    true
}

pub async fn download_libraries(
    libraries: &[Library],
    libraries_dir: &Path,
    concurrency: usize,
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
        if dest.exists() {
            continue;
        }
        tasks.push((artifact.url.clone(), dest));
    }

    if tasks.is_empty() {
        return Ok(());
    }

    let sem = Arc::new(Semaphore::new(concurrency));
    let mut errors = Vec::new();
    let mut handles = Vec::new();

    for (url, dest) in tasks {
        let permit = sem.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit.acquire_owned().await.unwrap();
            fetcher::fetch(&url, &dest, None::<fn(u64, u64)>)
                .await
                .map_err(|e| format!("{}: {}", dest.display(), e))
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
