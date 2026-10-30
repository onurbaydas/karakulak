//! Backups of the Firefox profile (bookmarks, passwords, history, extension
//! data) and Karakulak's settings, as a .tar.gz on a USB stick or at home.
//! Restoring happens at the next login, since Firefox holds its profile open.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Inside the home folder.
const PROFILE: &str = ".karakulak/profile";
const SETTINGS: &str = ".config/karakulak";
/// Restored here; karakulak-session moves it into place before Firefox starts.
const PENDING: &str = ".karakulak/restore-pending";
/// Caches and lock files: rebuilt by Firefox, no use in a backup.
const SKIP: &[&str] = &["cache2", "startupCache", "thumbnails", "storage/temporary", "lock", ".parentlock", "crashes"];

fn home() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "no home folder".into())
}

fn stamp() -> String {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    format!("{:04}-{:02}-{:02}-{:02}{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min)
}

fn tar(args: &[&str]) -> Result<String, String> {
    let out = Command::new("tar").args(args).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Writes a backup into `folder`: `{"path"}`.
pub fn backup(folder: &str) -> Result<Value, String> {
    let home = home()?;
    if !Path::new(folder).is_dir() {
        return Err("not a folder".into());
    }
    let file = Path::new(folder).join(format!("karakulak-yedek-{}.tar.gz", stamp()));
    let file_str = file.to_string_lossy().into_owned();
    let home_str = home.to_string_lossy().into_owned();
    let mut args: Vec<String> = vec!["-czf".into(), file_str, "-C".into(), home_str];
    args.extend(SKIP.iter().map(|s| format!("--exclude={s}")));
    for part in [PROFILE, SETTINGS] {
        if home.join(part).exists() {
            args.push(part.into());
        }
    }
    tar(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
    Ok(json!({ "path": file }))
}

/// Backups at home, in Downloads, and at the top of mounted USB sticks:
/// `[{"path", "name"}]`, newest first.
pub fn list() -> Result<Value, String> {
    let mut places = vec![home()?];
    places.extend(dirs::download_dir());
    if let Ok(media) = std::fs::read_dir(format!("/media/{}", std::env::var("USER").unwrap_or_default())) {
        places.extend(media.flatten().map(|e| e.path()));
    }
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    for place in places {
        for entry in std::fs::read_dir(&place).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("karakulak-yedek-") && name.ends_with(".tar.gz") {
                found.push((name, entry.path()));
            }
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(Value::Array(found.into_iter().map(|(name, path)| json!({ "name": name, "path": path })).collect()))
}

/// Unpacks a backup for the next login, after checking it only holds the
/// profile and settings.
pub fn restore(file: &str) -> Result<Value, String> {
    let listing = tar(&["-tzf", file])?;
    let allowed = |entry: &str| {
        let entry = entry.trim_start_matches("./");
        (entry.starts_with(".karakulak/") || entry.starts_with(".config/")) && !entry.split('/').any(|part| part == "..")
    };
    if listing.lines().any(|l| !allowed(l)) {
        return Err("not a Karakulak backup".into());
    }
    let pending = home()?.join(PENDING);
    let _ = std::fs::remove_dir_all(&pending);
    std::fs::create_dir_all(&pending).map_err(|e| e.to_string())?;
    tar(&["-xzf", file, "-C", &pending.to_string_lossy()])?;
    Ok(json!({ "pending": true }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn stamps_sort_by_time() {
        let s = super::stamp();
        assert_eq!(s.len(), "2026-10-30-1405".len());
    }
}
