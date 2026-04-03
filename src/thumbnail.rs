use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};
use directories::ProjectDirs;
use tracing::{debug, error};
use image::ImageFormat;

pub struct ThumbnailManager;

impl ThumbnailManager {
    /// サムネイル保存用のキャッシュディレクトリを生成または取得します。
    pub fn get_cache_dir() -> Option<PathBuf> {
        ProjectDirs::from("", "", "rimze").map(|dirs| dirs.cache_dir().to_path_buf())
    }

    /// 指定されたファイルパスのサムネイル保存先パスを返します。
    pub fn get_thumbnail_path(file_path: &Path) -> Option<PathBuf> {
        let cache_dir = Self::get_cache_dir()?;
        
        // ファイルの絶対パスからハッシュを生成してファイル名にします。
        let abs_path = std::fs::canonicalize(file_path).unwrap_or_else(|_| file_path.to_path_buf());
        let mut hasher = Sha256::new();
        hasher.update(abs_path.to_string_lossy().as_bytes());
        let hash = hex::encode(hasher.finalize());
        
        Some(cache_dir.join(format!("{}.avif", hash)))
    }

    /// サムネイルが必要な場合に生成して保存します。
    /// すでに存在する場合は何もしません。
    pub fn ensure_thumbnail(file_path: PathBuf, image_data: Vec<u8>) {
        let thumb_path = match Self::get_thumbnail_path(&file_path) {
            Some(p) => p,
            None => return,
        };

        if thumb_path.exists() {
            return;
        }

        debug!("Generating thumbnail for {:?}", file_path);
        
        // 重い処理なのでブロッキングを避けるため、呼び出し側で spawn されていることを想定
        if let Ok(img) = image::load_from_memory(&image_data) {
            let thumbnail = img.thumbnail(256, 256);
            
            if let Some(parent) = thumb_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            match thumbnail.save_with_format(&thumb_path, ImageFormat::Avif) {
                Ok(_) => debug!("Saved thumbnail to {:?}", thumb_path),
                Err(e) => error!("Failed to save thumbnail: {}", e),
            }
        } else {
            error!("Failed to decode image for thumbnail: {:?}", file_path);
        }
    }
}
