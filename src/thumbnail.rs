use directories::ProjectDirs;
use image::ImageFormat;
use natural_sort_rs::NaturalSort;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tracing::{debug, error};

pub enum ThumbnailRequest {
    NewList(Vec<PathBuf>),
    SetFocus(usize),
    Stop,
}

pub struct ThumbnailWorker {
    tx: mpsc::UnboundedSender<ThumbnailRequest>,
}

impl ThumbnailWorker {
    pub fn spawn(rt: &tokio::runtime::Runtime) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<ThumbnailRequest>();

        rt.spawn(async move {
            let mut all_paths: Vec<PathBuf> = Vec::new();
            let mut focus_idx: usize;
            let mut pending_indices: Vec<usize> = Vec::new();

            loop {
                let has_pending = !pending_indices.is_empty();
                tokio::select! {
                    msg = rx.recv() => {
                        match msg {
                            Some(ThumbnailRequest::NewList(paths)) => {
                                all_paths = paths;
                                focus_idx = 0;
                                // すべてのパスをインデックスで管理
                                pending_indices = (0..all_paths.len()).collect();
                                Self::resort_pending(&mut pending_indices, focus_idx);
                            }
                            Some(ThumbnailRequest::SetFocus(idx)) => {
                                focus_idx = idx;
                                Self::resort_pending(&mut pending_indices, focus_idx);
                            }
                            Some(ThumbnailRequest::Stop) | None => break,
                        }
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(1)), if has_pending => {
                        // 次のパスを取り出して処理
                        if let Some(idx) = pending_indices.first().cloned() {
                            pending_indices.remove(0);
                            if let Some(path) = all_paths.get(idx) {
                                let path_clone = path.clone();
                                tokio::task::spawn_blocking(move || {
                                    ThumbnailManager::ensure_thumbnail_from_path(path_clone);
                                }).await.ok();
                            }
                        }
                    }
                }
            }
            debug!("Thumbnail worker stopped.");
        });

        Self { tx }
    }

    fn resort_pending(pending: &mut Vec<usize>, focus: usize) {
        pending.sort_by_key(|&i| (i as isize - focus as isize).abs());
    }

    pub fn new_list(&self, paths: Vec<PathBuf>) {
        let _ = self.tx.send(ThumbnailRequest::NewList(paths));
    }

    pub fn set_focus(&self, idx: usize) {
        let _ = self.tx.send(ThumbnailRequest::SetFocus(idx));
    }

    pub fn stop(&self) {
        let _ = self.tx.send(ThumbnailRequest::Stop);
    }
}

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

        Some(cache_dir.join(format!("{}.webp", hash)))
    }

    /// パスからサムネイルを生成して保存します。
    /// すでに存在する場合は何もしません。
    pub fn ensure_thumbnail_from_path(file_path: PathBuf) {
        let thumb_path = match Self::get_thumbnail_path(&file_path) {
            Some(p) => p,
            None => return,
        };

        if thumb_path.exists() {
            return;
        }

        debug!("Generating thumbnail from path for {:?}", file_path);

        let ext = file_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        let image_data = if ext == "zip" {
            Self::load_first_image_from_zip(&file_path)
        } else {
            std::fs::read(&file_path).ok()
        };

        if let Some(data) = image_data {
            Self::ensure_thumbnail(file_path, data);
        }
    }

    fn load_first_image_from_zip(zip_path: &Path) -> Option<Vec<u8>> {
        let file = std::fs::File::open(zip_path).ok()?;
        let mut archive = zip::ZipArchive::new(file).ok()?;

        let mut image_entries: Vec<String> = archive
            .file_names()
            .filter(|name| {
                !name.ends_with('/') && {
                    let lower = name.to_lowercase();
                    lower.ends_with(".png")
                        || lower.ends_with(".jpg")
                        || lower.ends_with(".jpeg")
                        || lower.ends_with(".webp")
                        || lower.ends_with(".gif")
                        || lower.ends_with(".avif")
                }
            })
            .map(|s| s.to_string())
            .collect();

        image_entries.natural_sort::<str>();

        if let Some(first_entry) = image_entries.first() {
            let mut zip_file = archive.by_name(first_entry).ok()?;
            let mut buffer = Vec::with_capacity(zip_file.size() as usize);
            zip_file.read_to_end(&mut buffer).ok()?;
            return Some(buffer);
        }
        None
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
            let thumbnail = img.thumbnail(64, 64);

            if let Some(parent) = thumb_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            match thumbnail.save_with_format(&thumb_path, ImageFormat::WebP) {
                Ok(_) => debug!("Saved thumbnail to {:?}", thumb_path),
                Err(e) => error!("Failed to save thumbnail: {}", e),
            }
        } else {
            error!("Failed to decode image for thumbnail: {:?}", file_path);
        }
    }
}
