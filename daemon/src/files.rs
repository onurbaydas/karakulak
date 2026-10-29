//! File operations for the file manager page. Files are opened by Firefox
//! itself (PDF, images, video, text), so there is no viewer here.

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

/// Native messaging replies are limited to 1 MB.
const MAX_ENTRIES: usize = 3000;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The user's folders: `[{"name", "path"}]`.
pub fn places() -> Value {
    let candidates = [
        ("Ev", dirs::home_dir()),
        ("İndirilenler", dirs::download_dir()),
        ("Belgeler", dirs::document_dir()),
        ("Resimler", dirs::picture_dir()),
        ("Müzik", dirs::audio_dir()),
        ("Videolar", dirs::video_dir()),
    ];
    let places: Vec<Value> = candidates
        .into_iter()
        .filter_map(|(name, path)| path.filter(|p| p.is_dir()).map(|p| json!({ "name": name, "path": p })))
        .collect();
    Value::Array(places)
}

/// `{"path", "parent", "entries": [{"name", "path", "dir", "size", "modified"}], "truncated"}`
/// Folders first, then by name. Hidden files only if asked for.
pub fn list(path: &str, hidden: bool) -> Result<Value, String> {
    let dir = PathBuf::from(path);
    let mut entries: Vec<(bool, String, Value)> = Vec::new();
    for entry in fs::read_dir(&dir).map_err(err)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !hidden && name.starts_with('.') {
            continue;
        }
        // Follows symlinks, so a link to a folder opens like a folder.
        let Ok(meta) = fs::metadata(entry.path()) else {
            continue;
        };
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        entries.push((
            meta.is_dir(),
            name.to_lowercase(),
            json!({
                "name": name,
                "path": entry.path(),
                "dir": meta.is_dir(),
                "size": meta.len(),
                "modified": modified,
            }),
        ));
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let truncated = entries.len() > MAX_ENTRIES;
    let entries: Vec<Value> = entries.into_iter().take(MAX_ENTRIES).map(|(_, _, v)| v).collect();
    Ok(json!({
        "path": dir,
        "parent": dir.parent(),
        "entries": entries,
        "truncated": truncated,
    }))
}

/// Opens a file in a new Firefox tab (KARAKULAK_OPEN overrides the command).
pub fn open(path: &str) -> Result<Value, String> {
    let url = format!("file://{}", encode_path(path));
    let command = std::env::var("KARAKULAK_OPEN").unwrap_or_else(|_| "firefox --new-tab".into());
    let mut parts = command.split_whitespace();
    let program = parts.next().ok_or("empty KARAKULAK_OPEN")?;
    Command::new(program).args(parts).arg(url).spawn().map_err(err)?;
    Ok(Value::Null)
}

/// Percent-encodes what a file URL cannot hold as-is.
fn encode_path(path: &str) -> String {
    let mut out = String::new();
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Rejects names that would leave the folder or be invalid.
fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        return Err("invalid name".into());
    }
    Ok(())
}

pub fn mkdir(parent: &str, name: &str) -> Result<Value, String> {
    check_name(name)?;
    fs::create_dir(Path::new(parent).join(name)).map_err(err)?;
    Ok(Value::Null)
}

pub fn rename(path: &str, name: &str) -> Result<Value, String> {
    check_name(name)?;
    let from = Path::new(path);
    let to = from.parent().ok_or("no parent folder")?.join(name);
    if to.exists() {
        return Err("a file with that name already exists".into());
    }
    fs::rename(from, to).map_err(err)?;
    Ok(Value::Null)
}

/// `name`, or `name (2)`, `name (3)`... if taken.
fn free_name(folder: &Path, name: &str) -> PathBuf {
    let candidate = folder.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (2..)
        .map(|n| folder.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap()
}

fn copy_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
    if fs::symlink_metadata(from)?.is_dir() {
        fs::create_dir(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to).map(|_| ())
    }
}

/// Copies or moves `paths` into the folder `to`, never overwriting.
pub fn transfer(paths: &[String], to: &str, keep_original: bool) -> Result<Value, String> {
    let folder = Path::new(to);
    if !folder.is_dir() {
        return Err("target is not a folder".into());
    }
    for path in paths {
        let from = Path::new(path);
        let name = from.file_name().ok_or("invalid path")?.to_string_lossy();
        if folder.starts_with(from) {
            return Err("cannot put a folder inside itself".into());
        }
        let target = free_name(folder, &name);
        if keep_original {
            copy_recursive(from, &target).map_err(err)?;
        } else if fs::rename(from, &target).is_err() {
            // Different disk (e.g. USB stick): copy, then remove.
            copy_recursive(from, &target).map_err(err)?;
            trash::delete(from).map_err(err)?;
        }
    }
    Ok(Value::Null)
}

/// Moves to the trash, so nothing is lost by a wrong click.
pub fn to_trash(paths: &[String]) -> Result<Value, String> {
    trash::delete_all(paths).map_err(err)?;
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_paths_for_urls() {
        assert_eq!(encode_path("/home/a b/ç#.pdf"), "/home/a%20b/%C3%A7%23.pdf");
    }

    #[test]
    fn refuses_names_that_escape() {
        assert!(check_name("../x").is_err());
        assert!(check_name("..").is_err());
        assert!(check_name("ok.txt").is_ok());
    }

    #[test]
    fn copies_and_never_overwrites() {
        let root = std::env::temp_dir().join(format!("karakulak-files-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src/sub")).unwrap();
        fs::create_dir_all(root.join("dst")).unwrap();
        fs::write(root.join("src/sub/a.txt"), "x").unwrap();
        fs::write(root.join("dst/a.txt"), "old").unwrap();

        let src = root.join("src/sub/a.txt").to_string_lossy().into_owned();
        transfer(&[src.clone()], &root.join("dst").to_string_lossy(), true).unwrap();
        assert_eq!(fs::read_to_string(root.join("dst/a.txt")).unwrap(), "old");
        assert_eq!(fs::read_to_string(root.join("dst/a (2).txt")).unwrap(), "x");

        let folder = root.join("src").to_string_lossy().into_owned();
        assert!(transfer(&[folder.clone()], &root.join("src/sub").to_string_lossy(), true).is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
