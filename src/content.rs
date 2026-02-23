use std::{path::PathBuf, time::SystemTime};
use std::collections::{HashMap, VecDeque, HashSet};
use std::io::Read;
use std::sync::{Arc, Mutex};
use eframe::egui::{self, ColorImage, Context};
use tokio::runtime::Runtime;
use zip::ZipArchive;
use natural_sort_rs::NaturalSort;
use tracing::debug;

/// 画像ファイルの拡張子を定義します。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageExtension {
    Png,
    Jpg,
    Jpeg,
    Webp,
    Gif,
}

impl ImageExtension {
    /// 画像ファイル拡張子のスライスを返します。
    pub fn as_slice() -> &'static [ImageExtension] {
        &[Self::Png, Self::Jpg, Self::Jpeg, Self::Webp, Self::Gif]
    }
    
    /// 拡張子の文字列表現を返します。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpg => "jpg",
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
            Self::Gif => "gif",
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
}

impl FileExtension {
    /// 全ファイル拡張子のスライスを返します。
    pub fn as_slice() -> &'static [FileExtension] {
        &[Self::Png, Self::Jpg, Self::Jpeg, Self::Webp, Self::Gif, Self::Zip, Self::Pdf]
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
#[derive(Debug, Default, Clone, PartialEq)]
pub enum SortType {
    /// ファイル名でソートします（デフォルト）。
    #[default]
    FileName,
    /// 更新日でソートします。
    ModifiedDate,
    /// 作成日でソートします。
    CreationDate,
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
    image_cache: Arc<Mutex<ImageCache>>,
}

impl ComicLoader {
    /// 新しい `ComicLoader` インスタンスを作成します。
    ///
    /// # 引数
    /// - `runtime`: 非同期ランタイムの `Arc`。
    /// - `image_cache`: 画像キャッシュの `Arc<Mutex>`。
    ///
    /// # 戻り値
    /// `Self`: 新しい `ComicLoader` インスタンス。
    pub fn new(runtime: Arc<Runtime>, image_cache: Arc<Mutex<ImageCache>>) -> Self {
        Self { runtime, image_cache }
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
    pub async fn load_comic_file(&self, path: PathBuf) -> Result<ComicFile, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let original_path = path.clone();
        let metadata = tokio::fs::metadata(&original_path).await?;
        let file_type = if metadata.is_dir() {
            FileType::Directory(Directory { path: original_path.clone(), files: vec![] })
        } else if original_path.extension().is_some_and(|ext| ext.to_string_lossy().to_lowercase() == "zip") {
            let path_clone = original_path.clone();
            let entries_result = tokio::task::spawn_blocking(move || -> Result<Vec<String>, std::io::Error> {
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
            }).await;

            let entries = entries_result??;

            FileType::Zip(ZipFile { path: original_path.clone(), entries })
        } else if original_path.extension().is_some_and(|ext| ext.to_string_lossy().to_lowercase() == "pdf") {
            FileType::Pdf(PdfFile { path: original_path.clone() })
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
    pub async fn list_directory_paths(&self, dir_path: &PathBuf, sort_type: &SortType) -> Result<Vec<PathBuf>, Box<dyn std::error::Error + Send + Sync>> {
        let mut paths = Vec::new();
        let mut entries = tokio::fs::read_dir(dir_path).await?;

        let mut handles = vec![];

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
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
                    files_with_meta.sort_by(|a, b| b.1.modified().unwrap_or(SystemTime::UNIX_EPOCH).cmp(&a.1.modified().unwrap_or(SystemTime::UNIX_EPOCH)));
                },
                SortType::CreationDate => {
                    files_with_meta.sort_by(|a, b| b.1.created().unwrap_or(SystemTime::UNIX_EPOCH).cmp(&a.1.created().unwrap_or(SystemTime::UNIX_EPOCH)));
                },
                _ => {},
            }
            paths = files_with_meta.into_iter().map(|(p, _)| p).collect();
        } else {
            paths.natural_sort_by_key::<str, _, _>(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string());
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
/// このキャッシュは、メモリ使用量を制限しながら、最近アクセスされた画像データを保持し、
/// プリフェッチ機能もサポートします。
pub struct ImageCache {
    /// `CacheKey` をキーとし、生の画像バイトデータを値とするハッシュマップ。
    cache: HashMap<CacheKey, Vec<u8>>,
    /// 現在キャッシュウィンドウ内にあるキーのセット。
    window: HashSet<CacheKey>,
    /// 現在のメモリ使用量（バイト単位）。
    current_memory_usage: usize,
    /// キャッシュの最大メモリ使用量（バイト単位）。
    max_memory_usage: usize,
    /// 現在のページから前方（次へ）にプリフェッチするウィンドウサイズ。
    window_size_next: usize,
    /// 現在のページから後方（前へ）にプリフェッチするウィンドウサイズ。
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
            cache: HashMap::new(),
            window: HashSet::new(),
            current_memory_usage: 0,
            max_memory_usage,
            window_size_next: 10,
            window_size_prev: 5,
        }
    }
    
    /// 指定されたキーに対応する画像データをキャッシュから取得します。
    ///
    /// # 引数
    /// - `key`: 取得するデータの `CacheKey` への参照。
    ///
    /// # 戻り値
    /// `Option<Vec<u8>>`: 取得された画像データのバイトベクトル（クローン）、またはキーが存在しない場合は `None`。
    pub fn get(&self, key: &CacheKey) -> Option<Vec<u8>> {
        self.cache.get(key).cloned()
    }

    /// 画像データをキャッシュに挿入します。
    ///
    /// この関数は、指定されたキーと画像データをキャッシュに格納します。
    /// キャッシュが既にキーを含んでいる場合や、メモリ制限を超過する場合は挿入されません。
    ///
    /// # 引数
    /// - `key`: 挿入するデータの `CacheKey`。
    /// - `image_data`: 挿入する画像のバイトデータ。
    ///
    /// # 動作
    /// 1. キャッシュが既に `key` を含んでいる場合、何もしません。
    /// 2. 挿入するデータのサイズを計算します。
    /// 3. 現在のメモリ使用量と新しいデータのサイズが `max_memory_usage` を超える場合、
    ///    デバッグログを出力し、挿入せずに終了します。
    /// 4. `current_memory_usage` を更新し、`key` と `image_data` をキャッシュに挿入します。
    /// 5. `key` をキャッシュウィンドウ (`self.window`) に追加します。
    fn insert(&mut self, key: CacheKey, image_data: Vec<u8>) {
        if self.cache.contains_key(&key) { return; }
        let size_in_bytes = image_data.len();
        if self.current_memory_usage + size_in_bytes > self.max_memory_usage {
             debug!("Cache memory limit reached. Cannot insert {:?}", key);
             return;
        }
        self.current_memory_usage += size_in_bytes;
        self.cache.insert(key.clone(), image_data);
        self.window.insert(key);
    }
    
    /// 指定されたキーに対応するデータをキャッシュから削除します。
    ///
    /// # 引数
    /// - `key`: 削除するデータの `CacheKey` への参照。
    ///
    /// # 動作
    /// 1. キャッシュから `key` に対応するデータを削除します。
    /// 2. データが削除された場合、`current_memory_usage` を削除されたデータのサイズ分減らします。
    /// 3. `key` をキャッシュウィンドウ (`self.window`) から削除します。
    /// 4. デバッグログを出力します。
    fn evict(&mut self, key: &CacheKey) {
        if let Some(removed_data) = self.cache.remove(key) {
            self.current_memory_usage -= removed_data.len();
            self.window.remove(key);
            debug!("Evicted {:?} from cache.", key);
        }
    }
    
    /// キャッシュウィンドウを更新し、プリフェッチが必要なキーのリストを返します。
    ///
    /// この関数は、現在の表示キー (`center_key`) を中心に、
    /// `window_size_prev` と `window_size_next` に基づいて新しいキャッシュウィンドウを計算します。
    /// ウィンドウ外に出た既存のキャッシュエントリーは削除され、
    /// 新しいウィンドウ内にあるがキャッシュに存在しないキーがプリフェッチ対象として返されます。
    ///
    /// # 引数
    /// - `center_key`: 現在表示されている画像の `CacheKey` への参照。
    /// - `all_keys`: すべての可能な `CacheKey` の順序付きリストへのスライス。
    ///
    /// # 戻り値
    /// `Vec<CacheKey>`: プリフェッチが必要な `CacheKey` のリスト。
    ///
    /// # 動作
    /// 1. `all_keys` 内で `center_key` のインデックスを検索します。見つからない場合は空のベクトルを返します。
    /// 2. `center_idx`、`window_size_prev`、`window_size_next` に基づいて、
    ///    新しいキャッシュウィンドウの開始インデックス (`start`) と終了インデックス (`end`) を計算します。
    /// 3. `all_keys` のスライスから新しいウィンドウ内のキーを抽出し、`HashSet` (`new_window_keys`) に変換します。
    /// 4. 現在のウィンドウ (`self.window`) と `new_window_keys` の差分を取り、
    ///    ウィンドウ外に出たキー (`keys_to_evict`) を特定します。
    /// 5. `keys_to_evict` 内の各キーに対して `evict` を呼び出し、キャッシュから削除します。
    /// 6. `new_window_keys` 内のキーのうち、まだキャッシュに存在しないものだけをフィルタリングし、
    ///    そのリストを返します。これがプリフェッチが必要なキーのリストになります。
    pub fn update_window(&mut self, center_key: &CacheKey, all_keys: &[CacheKey]) -> Vec<CacheKey> {
        let Some(center_idx) = all_keys.iter().position(|k| k == center_key) else {
            return vec![];
        };

        let start = center_idx.saturating_sub(self.window_size_prev);
        let end = (center_idx + self.window_size_next).min(all_keys.len().saturating_sub(1));
        
        if start > end { return vec![]; }
        
        let new_window_keys: HashSet<CacheKey> = all_keys[start..=end].iter().cloned().collect();
        let keys_to_evict: Vec<CacheKey> = self.window.difference(&new_window_keys).cloned().collect();
        for key in keys_to_evict {
            self.evict(&key);
        }

        new_window_keys.into_iter()
            .filter(|k| !self.cache.contains_key(k))
            .collect()
    }
    
    /// キャッシュの最大メモリ使用量を設定します。
    ///
    /// # 引数
    /// - `bytes`: 新しい最大メモリ使用量（バイト単位）。
    ///
    /// # 動作
    /// 1. `self.max_memory_usage` を指定された値に更新します。
    /// 2. TODO: ここで、もし現在のメモリ使用量が新しい上限を超えている場合、
    ///    キャッシュからデータを削除する処理を実装する必要があります。
    pub fn set_max_memory_usage(&mut self, bytes: usize) {
        self.max_memory_usage = bytes;
        // TODO: ここでメモリが上限を超えていたら削除処理を走らせる
    }

    /// キャッシュの内容をすべてクリアします。
    ///
    /// # 動作
    /// 1. `self.cache` をクリアします。
    /// 2. `self.window` をクリアします。
    /// 3. `self.current_memory_usage` を0にリセットします。
    pub fn clear(&mut self) {
        self.cache.clear();
        self.window.clear();
        self.current_memory_usage = 0;
    }
    
    /// プリフェッチされた画像データをキャッシュに挿入します。
    ///
    /// この関数は、プリフェッチされたデータが現在のキャッシュウィンドウ内にあり、
    /// かつまだキャッシュに存在しない場合にのみデータを挿入します。
    ///
    /// # 引数
    /// - `key`: 挿入するデータの `CacheKey`。
    /// - `data`: 挿入する画像のバイトデータ。
    ///
    /// # 動作
    /// 1. `key` が現在のキャッシュウィンドウ (`self.window`) に含まれており、
    ///    かつキャッシュ (`self.cache`) にまだ存在しないことを確認します。
    /// 2. 条件が満たされた場合、`insert` メソッドを呼び出してデータをキャッシュに格納します。
    pub fn insert_prefetched_data(&mut self, key: CacheKey, data: Vec<u8>) {
        if self.window.contains(&key) && !self.cache.contains_key(&key) {
            self.insert(key, data);
        }
    }
}