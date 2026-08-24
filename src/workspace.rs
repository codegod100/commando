use std::fs;
use std::path::{Path, PathBuf};

use crate::agent::tools::format_size;

#[derive(Debug, Clone)]
pub struct FileItem {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size_label: String,
}

impl FileItem {
    pub fn icon_name(&self) -> &'static str {
        if self.is_dir {
            "folder-symbolic"
        } else {
            match self
                .path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" => "image-x-generic-symbolic",
                "mp3" | "ogg" | "wav" | "flac" => "audio-x-generic-symbolic",
                "mp4" | "webm" | "mkv" => "video-x-generic-symbolic",
                "pdf" => "x-office-document-symbolic",
                "rs" | "py" | "js" | "ts" | "go" | "c" | "h" | "toml" | "json" | "md" => {
                    "text-x-script-symbolic"
                }
                _ => "text-x-generic-symbolic",
            }
        }
    }

    pub fn is_image(&self) -> bool {
        matches!(
            self.path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg"
        )
    }
}

pub fn list_workspace(path: &Path) -> Result<Vec<FileItem>, String> {
    let mut items = Vec::new();
    let entries = fs::read_dir(path).map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
    for entry in entries.flatten() {
        let meta = match entry.metadata() {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        items.push(FileItem {
            path: entry.path(),
            name,
            is_dir: meta.is_dir(),
            size_label: if meta.is_dir() {
                String::new()
            } else {
                format_size(meta.len())
            },
        });
    }
    items.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(items)
}
