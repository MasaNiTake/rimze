use eframe::egui::{self, ColorImage, Context};
use natural_sort_rs::NaturalSort;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::sync::Arc;
use std::{path::PathBuf, time::SystemTime};
use tokio::runtime::Runtime;
use tracing::debug;
use zip::ZipArchive;

/// 画像ファイルの拡張子を定義します。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageExtension {
    Png,
    Jpg,
    Jpeg,
    Webp,
    Gif,
    Avif,
}

impl ImageExtension {
    /// 画像ファイル拡張子のスライスを返します。
    pub fn as_slice() -> &'static [ImageExtension] {
        &[
            Self::Png,
            Self::Jpg,
            Self::Jpeg,
            Self::Webp,
            Self::Gif,
            Self::Avif,
        ]
    }

    /// 拡張子の文字列表現を返します。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpg => "jpg",
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
            Self::Gif => "gif",
            Self::Avif => "avif",
        }
    }

    /// 文字列から拡張子をパースします。
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" => Some(Self::Jpg),
            "jpeg" => Some(Self::Jpeg),
            "webp" => Some(Self::Webp),
            "gif" => Some(Self::Gif),
            "avif" => Some(Self::Avif),
            _ => None,
        }
    }
}

/// サポートされている全ファイルの拡張子を定義します。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileExtension {
    Png,
    Jpg,
    Jpeg,
    Webp,
    Gif,
    Zip,
    Pdf,
    Avif,
}

impl FileExtension {
    /// 全ファイル拡張子のスライスを返します。
    pub fn as_slice() -> &'static [FileExtension] {
        &[
            Self::Png,
            Self::Jpg,
            Self::Jpeg,
            Self::Webp,
            Self::Gif,
            Self::Zip,
            Self::Pdf,
            Self::Avif,
        ]
    }

    /// 拡張子の文字列表現を返します。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpg => "jpg",
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
            Self::Gif => "gif",
            Self::Zip => "zip",
            Self::Pdf => "pdf",
            Self::Avif => "avif",
        }
    }

    /// 文字列から拡張子をパースします。
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" => Some(Self::Jpg),
            "jpeg" => Some(Self::Jpeg),
            "webp" => Some(Self::Webp),
            "gif" => Some(Self::Gif),
            "zip" => Some(Self::Zip),
            "pdf" => Some(Self::Pdf),
            "avif" => Some(Self::Avif),
            _ => None,
        }
    }
}

/// 漫画ビューアで扱うファイルの種類を定義します。
///
/// この列挙型は、アプリケーションがサポートする様々なコンテンツタイプを表します。
#[derive(Debug, Default, Clone)]
pub enum FileType {
    /// 未知のファイルタイプ。デフォルト値として使用されます。
    #[default]
    Unknown,
    /// 画像ファイル。`ImageFile` 構造体に関連情報が含まれます。
    Image(ImageFile),
    /// ZIPアーカイブファイル。`ZipFile` 構造体に関連情報が含まれます。
    Zip(ZipFile),
    /// PDFファイル。`PdfFile` 構造体に関連情報が含まれます。
    Pdf(PdfFile),
    /// ディレクトリ。`Directory` 構造体に関連情報が含まれます。
    Directory(Directory),
}

/// 画像ファイルに関する情報を保持する構造体です。
///
/// 画像のパスと、オプションで生の画像データを含みます。
#[derive(Debug, Default, Clone)]
pub struct ImageFile {
    /// 画像ファイルのパス。
    pub path: PathBuf,
    /// 画像の生のバイトデータ。ロードされていない場合は `None`。
    pub image_data: Option<Vec<u8>>,
}

impl ImageFile {
    /// Eguiで表示するための`ColorImage`を取得します。
    ///
    /// この関数は、`image_data` が存在する場合、それを `image` クレートでデコードし、
    /// Eguiが描画できる `ColorImage` 形式に変換して返します。
    ///
    /// # 戻り値
    /// `Option<ColorImage>`: 変換された `ColorImage`。画像データがないかデコードに失敗した場合は `None`。
    ///
    /// # 動作
    /// 1. `self.image_data` が `Some` であることを確認します。
    /// 2. `image::load_from_memory` を使用して生の画像データをデコードします。
    /// 3. デコードが成功した場合、画像の幅と高さを取得し、RGBA8形式に変換します。
    /// 4. `egui::ColorImage::from_rgba_unmultiplied` を使用して `ColorImage` を作成し、`Some` でラップして返します。
    /// 5. デコードに失敗した場合、`None` を返します。
    pub fn get_egui_color_image(&self) -> Option<ColorImage> {
        self.image_data.as_ref().and_then(|raw_img| {
            if let Ok(img) = image::load_from_memory(raw_img) {
                let size = [img.width() as _, img.height() as _];
                let image_buffer = img.to_rgba8();
                Some(ColorImage::from_rgba_unmultiplied(
                    size,
                    image_buffer.as_flat_samples().as_slice(),
                ))
            } else {
                None
            }
        })
    }
}

/// ZIPファイルに関する情報を保持する構造体です。
///
/// ZIPファイルのパスと、そのアーカイブ内のエントリー（通常は画像ファイル）のリストを含みます。
#[derive(Debug, Default, Clone)]
pub struct ZipFile {
    /// ZIPファイルのパス。
    pub path: PathBuf,
    /// ZIPアーカイブ内のエントリー名のリスト。
    pub entries: Vec<String>,
}

/// PDFファイルに関する情報を保持する構造体です。
///
/// PDFファイルのパスを含みます。
#[derive(Debug, Default, Clone)]
pub struct PdfFile {
    /// PDFファイルのパス。
    pub path: PathBuf,
}

/// ディレクトリに関する情報を保持する構造体です。
///
/// ディレクトリのパスと、そのディレクトリ内のファイルパスのリストを含みます。
#[derive(Debug, Default, Clone)]
pub struct Directory {
    /// ディレクトリのパス。
    pub path: PathBuf,
    /// ディレクトリ内のファイルパスのリスト。
    pub files: Vec<PathBuf>,
}
/// ファイルのソート順を定義します。
///
/// この列挙型は、ディレクトリ内のファイルをリストアップする際に使用されるソート基準を表します。
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub enum SortType {
    /// ファイル名でソートします（デフォルト）。
    #[default]
    FileName,
    /// 更新日でソートします。
    ModifiedDate,
    /// 作成日でソートします。
    CreationDate,
}

/// ファイルのソート順（昇順・降順）を定義します。
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub enum SortOrder {
    /// 昇順（デフォルト）。新しいもの、または文字コード順。
    #[default]
    Ascending,
    /// 降順。古いもの、または逆順。
    Descending,
}

/// 漫画ファイル（画像、ZIP、PDF、ディレクトリ）に関する共通情報を保持する構造体です。
///
/// ファイルのパス、タイプ、およびタイムスタンプ情報を含みます。
#[derive(Debug, Default, Clone)]
pub struct ComicFile {
    /// ファイルのパス。
    pub path: PathBuf,
    /// ファイルの具体的なタイプ（`FileType` 列挙型）。
    pub file_type: FileType,
    /// ファイルの最終更新日時。
    pub modified_date: Option<SystemTime>,
    /// ファイルの作成日時。
    pub creation_date: Option<SystemTime>,
}

impl ComicFile {
    /// この漫画ファイルの `FileType` への参照を返します。
    ///
    /// # 戻り値
    /// `&FileType`: ファイルのタイプへの参照。
    pub fn get_file_type(&self) -> &FileType {
        &self.file_type
    }
}

/// 漫画ファイルを非同期で読み込み、デコードする役割を担う構造体です。
///
/// Tokioランタイムと画像キャッシュへの参照を保持し、
/// ファイルシステムやZIPアーカイブからのデータロードを処理します。
pub struct ComicLoader {
    /// 非同期タスクを実行するためのTokioランタイム。
    runtime: Arc<Runtime>,
    /// 画像データをキャッシュするためのミューテックス保護されたキャッシュ。
    // 非同期タスクから安全にロックするため tokio::sync::Mutex を使用する。
    #[allow(dead_code)]
    image_cache: Arc<tokio::sync::Mutex<ImageCache>>,
}

impl ComicLoader {
    /// 新しい `ComicLoader` インスタンスを作成します。
    ///
    /// # 引数
    /// - `runtime`: 非同期ランタイムの `Arc`。
    /// - `image_cache`: 画像キャッシュの `Arc<tokio::sync::Mutex>`。
    ///
    /// # 戻り値
    /// `Self`: 新しい `ComicLoader` インスタンス。
    pub fn new(runtime: Arc<Runtime>, image_cache: Arc<tokio::sync::Mutex<ImageCache>>) -> Self {
        Self {
            runtime,
            image_cache,
        }
    }

    /// 指定されたパスの漫画ファイルを非同期で読み込み、デコードします。
    ///
    /// この関数は、ファイルのメタデータを取得し、その拡張子や種類に基づいて
    /// `ComicFile` 構造体を構築します。ZIPファイルの場合、内部の画像エントリーをリストアップします。
    ///
    /// # 引数
    /// - `path`: ロードするファイルの `PathBuf`。
    ///
    /// # 戻り値
    /// `Result<ComicFile, Box<dyn std::error::Error + Send + Sync + 'static>>`:
    /// ロードされた `ComicFile`、またはエラーが発生した場合はエラーオブジェクト。
    ///
    /// # 動作
    /// 1. `tokio::fs::metadata` を使用してファイルのメタデータを非同期で取得します。
    /// 2. メタデータに基づいて `FileType` を決定します。
    ///    - ディレクトリの場合: `FileType::Directory` を作成します。
    ///    - ZIPファイルの場合: `tokio::task::spawn_blocking` を使用してブロッキングタスクでZIPアーカイブを開き、
    ///      内部の画像エントリー（png, jpg, jpeg, webp, gif）を抽出し、自然順でソートします。
    ///      その後、`FileType::Zip` を作成します。
    ///    - PDFファイルの場合: `FileType::Pdf` を作成します。
    ///    - サポートされている画像ファイル（png, jpg, jpeg, webp, gif）の場合:
    ///      `tokio::fs::read` で画像データを非同期で読み込み、`FileType::Image` を作成します。
    ///    - それ以外の場合: `FileType::Unknown` を設定します。
    /// 3. 取得した情報（パス、ファイルタイプ、更新日時、作成日時）を使用して `ComicFile` を構築し、`Ok` でラップして返します。
    pub async fn load_comic_file(
        &self,
        path: PathBuf,
    ) -> Result<ComicFile, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let original_path = path.clone();
        let metadata = tokio::fs::metadata(&original_path).await?;
        let file_type = if metadata.is_dir() {
            FileType::Directory(Directory {
                path: original_path.clone(),
                files: vec![],
            })
        } else if original_path
            .extension()
            .is_some_and(|ext| ext.to_string_lossy().to_lowercase() == "zip")
        {
            let path_clone = original_path.clone();
            let entries_result =
                tokio::task::spawn_blocking(move || -> Result<Vec<String>, std::io::Error> {
                    let file = std::fs::File::open(path_clone)?;
                    let archive = ZipArchive::new(file)?;
                    let mut image_entries: Vec<String> = archive
                        .file_names()
                        .filter(|name| {
                            !name.ends_with('/') && {
                                if let Some(ext) = name.split('.').last() {
                                    ImageExtension::from_str(&ext).is_some()
                                } else {
                                    false
                                }
                            }
                        })
                        .map(|s| s.to_string())
                        .collect();

                    image_entries.natural_sort::<str>();
                    Ok(image_entries)
                })
                .await;

            let entries = entries_result??;

            FileType::Zip(ZipFile {
                path: original_path.clone(),
                entries,
            })
        } else if original_path
            .extension()
            .is_some_and(|ext| ext.to_string_lossy().to_lowercase() == "pdf")
        {
            FileType::Pdf(PdfFile {
                path: original_path.clone(),
            })
        } else if original_path.extension().is_some_and(|ext| {
            let lower_ext = ext.to_string_lossy().to_lowercase();
            ImageExtension::from_str(&lower_ext).is_some()
        }) {
            let image_data = tokio::fs::read(&original_path).await.ok();
            FileType::Image(ImageFile {
                path: original_path.clone(),
                image_data,
            })
        } else {
            FileType::Unknown
        };

        Ok(ComicFile {
            path: original_path,
            file_type,
            modified_date: metadata.modified().ok(),
            creation_date: metadata.created().ok(),
        })
    }

    /// ZIPファイル内の指定されたエントリーから画像データを読み込みます。
    ///
    /// この関数は、ZIPファイルのパスとエントリー名を受け取り、
    /// `tokio::task::spawn_blocking` を使用してブロッキングI/O操作を実行し、
    /// 指定されたエントリーの生のバイトデータを抽出します。
    ///
    /// # 引数
    /// - `zip_path`: ZIPファイルの `PathBuf` への参照。
    /// - `entry_name`: ZIPアーカイブ内の読み込むエントリーの名前。
    ///
    /// # 戻り値
    /// `Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>`:
    /// 読み込まれた画像データのバイトベクトル、またはエラーが発生した場合はエラーオブジェクト。
    ///
    /// # 動作
    /// 1. `zip_path` と `entry_name` をクローンして、ブロッキングタスクのクロージャに移動させます。
    /// 2. `tokio::task::spawn_blocking` を使用して、ブロッキングI/O操作（ファイルオープン、ZIPアーカイブの読み込み）を
    ///    専用のスレッドプールで実行します。
    /// 3. クロージャ内で：
    ///    - `std::fs::File::open` でZIPファイルを開きます。
    ///    - `zip::ZipArchive::new` でZIPアーカイブを作成します。
    ///    - `archive.by_name` で指定されたエントリーを取得します。
    ///    - `zip_file.read_to_end` でエントリーの内容をバイトベクトルに読み込みます。
    /// 4. ブロッキングタスクの結果を待ち、その結果を返します。
    pub async fn load_image_from_zip(
        &self,
        zip_path: &PathBuf,
        entry_name: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let zip_path = zip_path.clone();
        let entry_name = entry_name.to_string();
        let result_from_closure = tokio::task::spawn_blocking(
            move || -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
                let file = std::fs::File::open(&zip_path)?;
                let mut archive = zip::ZipArchive::new(file)?;
                let mut zip_file = archive.by_name(&entry_name)?;
                let mut buffer = Vec::with_capacity(zip_file.size() as usize);
                zip_file.read_to_end(&mut buffer)?;
                Ok(buffer)
            },
        )
        .await?;
        result_from_closure
    }

    /// 指定されたディレクトリ内の漫画ファイルの「パス」をリストアップし、指定されたソート順で並べ替えます。
    ///
    /// この関数は、ディレクトリ内のエントリーを非同期で読み取り、
    /// サポートされているファイルタイプ（ディレクトリ、ZIP、PDF、画像）のみをフィルタリングし、
    /// 指定されたソートタイプに基づいて結果を並べ替えます。
    ///
    /// # 引数
    /// - `dir_path`: リストアップするディレクトリの `PathBuf` への参照。
    /// - `sort_type`: ファイルをソートするための `SortType` への参照。
    ///
    /// # 戻り値
    /// `Result<Vec<PathBuf>, Box<dyn std::error::Error + Send + Sync>>`:
    /// ソートされたファイルパスのリスト、またはエラーが発生した場合はエラーオブジェクト。
    ///
    /// # 動作
    /// 1. `tokio::fs::read_dir` を使用してディレクトリのエントリーを非同期で読み取ります。
    /// 2. 各エントリーをループし、パスと拡張子をチェックして、サポートされているファイルタイプのみを対象とします。
    /// 3. `sort_type` が `FileName` 以外の場合、ファイルのメタデータを非同期で取得するタスクをスポーンし、
    ///    そのハンドルを `handles` ベクトルに格納します。これにより、メタデータ取得が並行して行われます。
    /// 4. `sort_type` が `FileName` の場合、パスを直接 `paths` ベクトルに追加します。
    /// 5. `handles` が空でない場合（つまり、ファイル名以外のソートが要求された場合）：
    ///    - すべてのスポーンされたタスクの結果を待ち、パスとメタデータを収集します。
    ///    - `sort_type` に応じて `files_with_meta` をソートします（更新日または作成日）。
    ///    - ソートされたパスを `paths` ベクトルに格納します。
    /// 6. `handles` が空の場合（つまり、ファイル名ソートが要求された場合）、
    ///    `natural_sort_by_key` を使用してファイル名を自然順でソートします。
    /// 7. ソートされた `paths` ベクトルを `Ok` でラップして返します。
    pub async fn list_directory_paths(
        &self,
        dir_path: &PathBuf,
        sort_type: &SortType,
        sort_order: &SortOrder,
    ) -> Result<Vec<PathBuf>, Box<dyn std::error::Error + Send + Sync>> {
        let mut paths = Vec::new();
        let mut entries = tokio::fs::read_dir(dir_path).await?;

        let mut handles = vec![];

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if path.is_dir() || FileExtension::from_str(&ext).is_some() {
                if *sort_type != SortType::FileName {
                    handles.push(tokio::spawn(async move {
                        tokio::fs::metadata(&path).await.ok().map(|m| (path, m))
                    }));
                } else {
                    paths.push(path);
                }
            }
        }

        if !handles.is_empty() {
            let mut files_with_meta = Vec::new();
            for handle in handles {
                if let Ok(Some((path, meta))) = handle.await {
                    files_with_meta.push((path, meta));
                }
            }
            match sort_type {
                SortType::ModifiedDate => {
                    files_with_meta.sort_by(|a, b| {
                        a.1.modified()
                            .unwrap_or(SystemTime::UNIX_EPOCH)
                            .cmp(&b.1.modified().unwrap_or(SystemTime::UNIX_EPOCH))
                    });
                }
                SortType::CreationDate => {
                    files_with_meta.sort_by(|a, b| {
                        a.1.created()
                            .unwrap_or(SystemTime::UNIX_EPOCH)
                            .cmp(&b.1.created().unwrap_or(SystemTime::UNIX_EPOCH))
                    });
                }
                _ => {}
            }
            paths = files_with_meta.into_iter().map(|(p, _)| p).collect();
        } else {
            paths.natural_sort_by_key::<str, _, _>(|p| {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            });
        }

        if *sort_order == SortOrder::Descending {
            paths.reverse();
        }
        Ok(paths)
    }
}

/// キャッシュのキーを定義します。
///
/// この列挙型は、画像キャッシュ内の各エントリーを一意に識別するために使用されます。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CacheKey {
    /// ファイルパスに基づくキー（単一の画像ファイル用）。
    File(PathBuf),
    /// ZIPファイル内のエントリーに基づくキー（ZIPファイル内のページ用）。
    /// タプルは `(zip_path, page_index)` を表します。
    ZipEntry(PathBuf, usize), // (zip_path, page_index)
}

/// 画像キャッシュを管理する構造体です。
///
/// 真の LRU（Least Recently Used）キャッシュとして実装されており、
/// メモリ使用量を制限しながら最近アクセスされた画像データを保持します。
/// 画像データは `Arc<Vec<u8>>` で保持され、取得時は参照カウント増加のみで
/// フルコピーを回避します。プリフェッチ（前後ページの事前読み込み）もサポートします。
pub struct ImageCache {
    /// LRU 順序付きキャッシュ本体。
    /// `lru::LruCache::unbounded()`（エントリ数上限なし）で生成し、
    /// メモリ使用量ベースの evict は `insert` 側で自前制御する。
    cache: lru::LruCache<CacheKey, Arc<Vec<u8>>>,
    /// 現在のメモリ使用量（バイト単位）。
    current_memory_usage: usize,
    /// キャッシュの最大メモリ使用量（バイト単位）。
    max_memory_usage: usize,
    /// 現在のページから前方（次へ）にプリフェッチする範囲（ページ数）。
    window_size_next: usize,
    /// 現在のページから後方（前へ）にプリフェッチする範囲（ページ数）。
    window_size_prev: usize,
}

impl ImageCache {
    /// 新しい `ImageCache` インスタンスを作成します。
    ///
    /// # 引数
    /// - `max_memory_usage`: キャッシュが使用できる最大メモリ量（バイト単位）。
    ///
    /// # 戻り値
    /// `Self`: 新しい `ImageCache` インスタンス。
    pub fn new(max_memory_usage: usize) -> Self {
        Self {
            cache: lru::LruCache::unbounded(),
            current_memory_usage: 0,
            max_memory_usage,
            window_size_next: 10,
            window_size_prev: 5,
        }
    }

    /// 現在のキャッシュメモリ使用量（バイト単位）を返します。
    pub fn current_memory_usage(&self) -> usize {
        self.current_memory_usage
    }

    /// 指定されたキーに対応する画像データをキャッシュから取得します。
    ///
    /// `lru::LruCache::get` によりアクセス順序が更新され（MRU 化）、
    /// 戻り値は `Arc<Vec<u8>>` のクローン（参照カウント増加のみで O(1)）となるため、
    /// 画像データ全体のフルコピーは発生しません。
    ///
    /// # 引数
    /// - `key`: 取得するデータの `CacheKey` への参照。
    ///
    /// # 戻り値
    /// `Option<Arc<Vec<u8>>>`: 取得された画像データの `Arc`、またはキーが存在しない場合は `None`。
    pub fn get(&mut self, key: &CacheKey) -> Option<Arc<Vec<u8>>> {
        self.cache.get(key).cloned()
    }

    /// 画像データをキャッシュに挿入します。
    ///
    /// メモリ使用量上限に達している場合は、**LRU 古い順（`pop_lru`）に自動的に evict** してから
    /// 挿入します。これにより真の LRU 挙動（上限到達後も新規エントリが入り、古いものが追い出される）
    /// を実現します。既存エントリがある場合は一度削除してサイズを再計算します。
    ///
    /// # 引数
    /// - `key`: 挿入するデータの `CacheKey`。
    /// - `data`: 挿入する画像のバイトデータ（`Arc` で共有）。
    ///
    /// # 動作
    /// 1. 既存エントリがある場合は `pop` して `current_memory_usage` を減らす。
    /// 2. `current_memory_usage + size` が `max_memory_usage` を超える間、`pop_lru` で古い順に evict。
    /// 3. キャッシュが空でも単一エントリのサイズが上限を超える極端ケースはログして挿入をスキップ。
    /// 4. `current_memory_usage` を加算し、`put` で挿入。
    pub fn insert(&mut self, key: CacheKey, data: Arc<Vec<u8>>) {
        let size = data.len();
        // 既存エントリがある場合は一旦削除してサイズ調整（値の上書きを正しく処理）。
        if let Some(old) = self.cache.pop(&key) {
            self.current_memory_usage -= old.len();
        }
        // メモリ上限を超える場合は LRU 古い順に evict して空きを作る。
        while self.current_memory_usage + size > self.max_memory_usage {
            match self.cache.pop_lru() {
                Some((_, old_data)) => {
                    self.current_memory_usage -= old_data.len();
                    debug!(
                        "Evicted LRU entry to make room (freed {} bytes).",
                        old_data.len()
                    );
                }
                None => {
                    // キャッシュ空でも size > max の場合。1エントリも入らない極端ケースはログしてスキップ。
                    debug!(
                        "Cannot insert {:?}: size {} exceeds max {}",
                        key, size, self.max_memory_usage
                    );
                    return;
                }
            }
        }
        self.current_memory_usage += size;
        // put は pop 済みなので通常 None を返すが、念のため結果は破棄する。
        let _ = self.cache.put(key, data);
    }

    /// キャッシュの最大メモリ使用量を設定します。
    ///
    /// 上限を更新した後、現在のメモリ使用量が新しい上限を超えている場合は、
    /// **LRU 古い順（`pop_lru`）に即時 evict** して上限内に収めます。
    ///
    /// # 引数
    /// - `bytes`: 新しい最大メモリ使用量（バイト単位）。
    pub fn set_max_memory_usage(&mut self, bytes: usize) {
        self.max_memory_usage = bytes;
        // 上限を下げた場合、超過分を LRU 古い順に即時 evict する。
        while self.current_memory_usage > self.max_memory_usage {
            match self.cache.pop_lru() {
                Some((_, old_data)) => {
                    self.current_memory_usage -= old_data.len();
                    debug!(
                        "Evicted LRU entry on max_memory_usage shrink (freed {} bytes).",
                        old_data.len()
                    );
                }
                None => break,
            }
        }
    }

    /// キャッシュの内容をすべてクリアします。
    pub fn clear(&mut self) {
        self.cache.clear();
        self.current_memory_usage = 0;
    }

    /// 現在表示中のキーを中心に、プリフェッチ対象のキーリストを計算して返します。
    ///
    /// `window_size_prev` / `window_size_next` に基づき `center_key` 前後の範囲を計算し、
    /// そのうち **まだキャッシュに存在しないキーのみ** を返します。
    /// 存在判定には `peek`（LRU 順序を更新しない読み取り専用アクセス）を使用するため、
    /// プリフェッチ判定自体が LRU 順序に影響を与えることはありません。
    ///
    /// # 引数
    /// - `center_key`: 現在表示されている画像の `CacheKey` への参照。
    /// - `all_keys`: すべての可能な `CacheKey` の順序付きリストへのスライス。
    ///
    /// # 戻り値
    /// `Vec<CacheKey>`: プリフェッチが必要な（未キャッシュの）`CacheKey` のリスト。
    pub fn compute_prefetch_keys(
        &self,
        center_key: &CacheKey,
        all_keys: &[CacheKey],
    ) -> Vec<CacheKey> {
        let Some(center_idx) = all_keys.iter().position(|k| k == center_key) else {
            return vec![];
        };

        let start = center_idx.saturating_sub(self.window_size_prev);
        let end = (center_idx + self.window_size_next).min(all_keys.len().saturating_sub(1));

        if start > end {
            return vec![];
        }

        // peek は LRU 順序を更新しないため、プリフェッチ判定に安全に使用できる。
        all_keys[start..=end]
            .iter()
            .filter(|k| self.cache.peek(*k).is_none())
            .cloned()
            .collect()
    }

    /// プリフェッチされた画像データをキャッシュに挿入します。
    ///
    /// プリフェッチ対象かどうかの判定は [`compute_prefetch_keys`](Self::compute_prefetch_keys) 側で
    /// 済んでいる前提で、ここでは単に [`insert`](Self::insert) に委譲します。
    /// LRU のメモリ上限管理（古い順 evict）は `insert` 内で行われます。
    ///
    /// # 引数
    /// - `key`: 挿入するデータの `CacheKey`。
    /// - `data`: 挿入する画像のバイトデータ（`Arc` で共有）。
    pub fn insert_prefetched_data(&mut self, key: CacheKey, data: Arc<Vec<u8>>) {
        self.insert(key, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用に連番の `CacheKey::File` を作成するヘルパ。
    fn key(n: u32) -> CacheKey {
        CacheKey::File(std::path::PathBuf::from(format!("/tmp/{}.png", n)))
    }

    /// 指定バイト数のダミーデータを `Arc<Vec<u8>>` で作成するヘルパ。
    fn data(size: usize) -> Arc<Vec<u8>> {
        Arc::new(vec![0u8; size])
    }

    #[test]
    fn test_lru_eviction_on_insert() {
        // 上限 100 バイト。各エントリ 30 バイト。
        // 3 エントリ（計 90）までは入る。4 つ目で最古が evict される。
        let mut cache = ImageCache::new(100);
        cache.insert(key(1), data(30));
        cache.insert(key(2), data(30));
        cache.insert(key(3), data(30));
        assert_eq!(cache.current_memory_usage(), 90);

        // さらに挿入。最古の key(1) が evict され、全体は 90 バイトを維持するはず。
        cache.insert(key(4), data(30));
        assert_eq!(cache.current_memory_usage(), 90);
        assert!(
            cache.get(&key(1)).is_none(),
            "key(1) は LRU 古い順に evict 済み"
        );
        assert!(cache.get(&key(2)).is_some());
        assert!(cache.get(&key(3)).is_some());
        assert!(cache.get(&key(4)).is_some());
    }

    #[test]
    fn test_get_updates_lru_order() {
        // 上限 90 バイト。3 エントリ × 30 バイトでぴったり。
        let mut cache = ImageCache::new(90);
        cache.insert(key(1), data(30));
        cache.insert(key(2), data(30));
        cache.insert(key(3), data(30));

        // key(1) をアクセスして MRU 化する（LRU 順序が更新されることの検証）。
        let _ = cache.get(&key(1));

        // 新規挿入で evict が走る。最古は key(2) のはず（key(1) はアクセス済みで MRU 側）。
        cache.insert(key(4), data(30));
        assert!(
            cache.get(&key(2)).is_none(),
            "key(2) が LRU 古い順に evict されるはず"
        );
        assert!(
            cache.get(&key(1)).is_some(),
            "key(1) は get 済みのため残るはず"
        );
        assert!(cache.get(&key(4)).is_some());
    }

    #[test]
    fn test_set_max_memory_usage_evicts() {
        let mut cache = ImageCache::new(120);
        cache.insert(key(1), data(30));
        cache.insert(key(2), data(30));
        cache.insert(key(3), data(30));
        assert_eq!(cache.current_memory_usage(), 90);

        // 上限を 50 に下げる。超過分（90 > 50）を LRU 古い順に即時 evict。
        // 30 バイト×1つ evict -> 60（まだ > 50）。もう1つ evict -> 30（<= 50）。
        cache.set_max_memory_usage(50);
        assert!(
            cache.current_memory_usage() <= 50,
            "上限下げ後に即時 evict され、使用量は新しい上限以下になるはず"
        );
        // key(1), key(2) が evict され、最も新しい key(3) が残るはず。
        assert!(cache.get(&key(1)).is_none());
        assert!(cache.get(&key(2)).is_none());
        assert!(cache.get(&key(3)).is_some());
    }

    #[test]
    fn test_compute_prefetch_keys() {
        // デフォルトの window_size_prev=5, window_size_next=10。
        let mut cache = ImageCache::new(1000);
        let all_keys: Vec<CacheKey> = (0..20).map(key).collect();
        let center = key(10);

        // ウィンドウ内のいくつかのキーを事前にキャッシュに投入（center 含む）。
        cache.insert(key(8), data(10));
        cache.insert(key(9), data(10));
        cache.insert(key(10), data(10)); // center 自体

        let prefetch = cache.compute_prefetch_keys(&center, &all_keys);

        // ウィンドウ範囲: start = 10 - 5 = 5, end = min(10 + 10, 19) = 19（添字 5..=19）。
        // 既存キャッシュ（key(8), key(9), key(10)）は除外される。
        assert!(!prefetch.contains(&key(8)), "既存キーは除外される");
        assert!(!prefetch.contains(&key(9)), "既存キーは除外される");
        assert!(
            !prefetch.contains(&key(10)),
            "center 自体も既存なので除外される"
        );

        // 範囲外（例: key(2)）は含まれない。
        assert!(!prefetch.contains(&key(2)), "ウィンドウ範囲外は含まれない");

        // 範囲内で未キャッシュのキーは含まれる。
        assert!(prefetch.contains(&key(5)), "範囲内・未キャッシュは含まれる");
        assert!(prefetch.contains(&key(19)), "範囲の右端も含まれる");
    }
}
