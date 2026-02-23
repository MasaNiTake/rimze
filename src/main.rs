use std::{path::PathBuf, sync::{Arc, Mutex}};
use eframe::egui;
use tokio::runtime;
use tracing::debug;
use std::sync::mpsc::{Sender, Receiver};

mod content;
mod view;

use content::{CacheKey, ComicFile, FileType, SortType, Directory, ImageExtension, FileExtension};
use view::UiCommand;


/// アプリケーションのエントリーポイント。
/// Eguiアプリケーションを初期化し、実行します。
fn main() -> Result<(), eframe::Error> {
    // tracingサブスクライバーを設定し、エラーレベル以上のログを出力します。
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    // Eframeのネイティブオプションを設定します。
    let options = eframe::NativeOptions {
    viewport: egui::ViewportBuilder::default()
    .with_inner_size([800.0, 600.0])
    .with_drag_and_drop(true)
    ,
    ..Default::default()
    };
    // Eframeアプリケーションを実行します。
    eframe::run_native(
        "Image viewer",
        options,
        Box::new(|cc| Ok(Box::new(MyApp::new(cc)) as Box<dyn eframe::App>)),
    )
}

/// アプリケーションのメイン状態を保持する構造体です。
struct MyApp {
    dropped_files: Vec<egui::DroppedFile>,
    content_file: Option<content::ComicFile>,
    current_image_handle: Option<egui::TextureHandle>,
    sort_files: content::SortType,
    directory: Option<content::Directory>,
    parent_directory: Option<content::Directory>,
    max_load_use_memory: usize,
    tokio_rt: Arc<runtime::Runtime>,
    comic_loader: Arc<content::ComicLoader>,
    image_cache: Arc<Mutex<content::ImageCache>>,
    current_page_index: usize,
    current_directory_path: Option<PathBuf>,
    ui_state: view::ComicViewerUI,
    update_tx: std::sync::mpsc::Sender<UiUpdateMsg>,
    update_rx: std::sync::mpsc::Receiver<UiUpdateMsg>,
    last_error: Option<String>,
}

// UI構築のために必要なアプリケーション状態をまとめた構造体
pub struct ComicViewerAppState<'a> {
    pub content_file: &'a mut Option<content::ComicFile>,
    pub current_image_handle: &'a mut Option<egui::TextureHandle>,
    pub sort_files: &'a mut content::SortType,
    pub max_load_use_memory: &'a mut usize,
    pub directory: &'a Option<content::Directory>,
    pub current_page_index: &'a mut usize,
}

/// UIの更新メッセージを定義します。
pub enum UiUpdateMsg {
    ComicFileLoaded(content::ComicFile, InitialPage),
    DirectoryLoaded(content::Directory),
    ParentDirectoryLoaded(content::Directory),
    ImageLoaded(egui::ColorImage),
    DirectoryChanged(content::Directory),
    DirectoryChangedFromDrop(content::Directory),
    Error(String),
}

/// 読み込み後の初期ページ指定
#[derive(Clone, Copy)]
pub enum InitialPage {
    First,
    Last,
}


impl eframe::App for MyApp {
    /// アプリケーションのUIを更新します。
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let mut app_state = ComicViewerAppState {
            content_file: &mut self.content_file,
            current_image_handle: &mut self.current_image_handle,
            sort_files: &mut self.sort_files,
            max_load_use_memory: &mut self.max_load_use_memory,
            directory: &self.directory,
            current_page_index: &mut self.current_page_index,
        };

        let commands = self.ui_state.build_ui(ctx, frame, &mut app_state);
        for command in commands {
            self.handle_ui_command(command);
        }

        self.ui_file_drag_and_drop(ctx);
        self.handle_image_navigation(ctx);

        while let Ok(msg) = self.update_rx.try_recv() {
            match msg {
                UiUpdateMsg::ComicFileLoaded(comic_file, initial_page) => {
                    debug!("Comic file loaded: {:?}", comic_file.path);
                    self.last_error = None;
                    self.open_comic_file(comic_file, initial_page);
                }
                UiUpdateMsg::DirectoryLoaded(directory) => {
                    debug!("Directory loaded: {:?}", directory.path);
                    self.directory = Some(directory);
                }
                UiUpdateMsg::ParentDirectoryLoaded(directory) => {
                    debug!("Parent directory loaded: {:?}", directory.path);
                    self.parent_directory = Some(directory);
                }
                UiUpdateMsg::ImageLoaded(color_image) => {
                    self.last_error = None;
                    self.current_image_handle = Some(ctx.load_texture(
                        "current_image",
                        color_image,
                        egui::TextureOptions::default(),
                    ));
                }
                UiUpdateMsg::DirectoryChanged(directory) => {
                    debug!("Directory changed: {:?}", directory.path);
                    self.directory = Some(directory);
                }
                UiUpdateMsg::DirectoryChangedFromDrop(directory) => {
                    debug!("Directory changed from drop: {:?}", directory.path);
                    self.directory = Some(directory);
                    if let Some(dir) = &self.directory {
                        if let Some(first_file) = dir.files.iter().find(|path| {
                            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                            ImageExtension::from_str(&ext).is_some()
                        }) {
                            debug!("Auto-opening first file in new directory: {:?}", first_file);
                            self.open_new_file(first_file.clone());
                        } else {
                            debug!("No image or zip files found in directory");
                        }
                    }
                }
                UiUpdateMsg::Error(err_msg) => {
                    eprintln!("Error: {}", err_msg);
                    self.last_error = Some(err_msg);
                }
            }
        }
        if let Some(error) = &self.last_error {
            egui::Area::new("error_toast".into())
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -20.0))
                .show(ctx, |ui| {
                    let frame = egui::Frame::popup(ui.style());
                    frame.show(ui, |ui| {
                        ui.label(egui::RichText::new(error).color(ui.style().visuals.error_fg_color));
                    });
                });
        }
    }
}

impl MyApp{
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (update_tx, update_rx) = std::sync::mpsc::channel();

        egui_extras::install_image_loaders(&cc.egui_ctx);
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert("ja_font".to_owned(),
            Arc::new(egui::FontData::from_static(include_bytes!("../fonts/PlemolJPConsoleNF-Regular.ttf"))));
        fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "ja_font".to_owned());
        fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().push("ja_font".to_owned());
        cc.egui_ctx.set_fonts(fonts);

        let tokio_rt = Arc::new(runtime::Builder::new_multi_thread().enable_all().build().unwrap());
        let max_memory_usage = 500 * 1024 * 1024;
        let image_cache = Arc::new(Mutex::new(content::ImageCache::new(max_memory_usage)));
        let comic_loader = Arc::new(content::ComicLoader::new(tokio_rt.clone(), image_cache.clone()));

        Self {
            dropped_files: Default::default(),
            content_file: None,
            current_image_handle: None,
            sort_files: Default::default(),
            directory: None,
            parent_directory:  None,
            max_load_use_memory: max_memory_usage,
            tokio_rt,
            comic_loader,
            image_cache,
            current_page_index: 0,
            current_directory_path: None,
            ui_state: view::ComicViewerUI::new(),
            update_tx,
            update_rx,
            last_error: None,
        }
    }

    /// UIから発行されたコマンドを処理します。
    fn handle_ui_command(&mut self, command: UiCommand) {
        match command {
            UiCommand::OpenFileDialog => {
                let file = rfd::FileDialog::new()
                    .add_filter("Image Files", &FileExtension::as_slice().iter().map(|ext| ext.as_str()).collect::<Vec<_>>())
                    .set_directory(self.ui_state.last_open_dir.as_deref().unwrap_or(&PathBuf::from("/")))
                    .pick_file();

                if let Some(path) = file {
                    self.ui_state.last_open_dir = path.parent().map(|p| p.to_path_buf());
                    self.open_new_file(path);
                }
            }
            UiCommand::OpenFile(path) => {
                self.load_and_open_path(path, InitialPage::First);
            }
            UiCommand::CloseFile => {
                self.content_file = None;
                self.current_image_handle = None;
                self.directory = None;
                self.parent_directory = None;
            }
            UiCommand::SetSort(sort_type) => {
                if self.sort_files != sort_type {
                    self.sort_files = sort_type;
                    if let Some(path) = self.current_directory_path.clone() {
                        self.load_directory_content(path, false);
                    }
                }
            }
            UiCommand::ChangePage(new_page) => {
                if self.current_page_index != new_page {
                    self.current_page_index = new_page;
                    self.load_image_for_display();
                }
            }
            UiCommand::SetMaxMemory(bytes) => {
                self.max_load_use_memory = bytes;
                self.image_cache.lock().unwrap().set_max_memory_usage(bytes);
            }
        }
    }

    /// ファイルのドラッグ＆ドロップUIを処理します。
    fn ui_file_drag_and_drop(&mut self, ctx: &egui::Context) {
        use egui::{Align2, Color32, Id, LayerId, Order, TextStyle};

        if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
            let text = ctx.input(|i| {
                let mut text = "Dropping files:\n".to_owned();
                for file in &i.raw.hovered_files {
                    if let Some(path) = &file.path {
                        use std::fmt::Write as _;
                        write!(text, "\n{}", path.display()).ok();
                    } else if !file.mime.is_empty() {
                        text += &file.mime;
                    } else {
                        text += "\n???";
                    }
                }
                text
            });
            let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("file_drop_target")));
            let screen_rect = ctx.screen_rect();
            painter.rect_filled(screen_rect, 0.0, Color32::from_black_alpha(192));
            painter.text(
                screen_rect.center(),
                Align2::CENTER_CENTER,
                text,
                TextStyle::Heading.resolve(&ctx.style()),
                Color32::WHITE,
            );
        }
        if !ctx.input(|i| i.raw.dropped_files.is_empty()) {
            self.dropped_files = ctx.input(|i| i.raw.dropped_files.clone());
            if let Some(file) = self.dropped_files.first() {
                if let Some(path) = &file.path {
                    if path.is_dir() {
                        debug!("Dropped directory: {:?}", path);
                        self.current_directory_path = Some(path.clone());

                        // ディレクトリを読み込んで自動オープンをトリガーするタスクを生成します。
                        let comic_loader = self.comic_loader.clone();
                        let tx = self.update_tx.clone();
                        let sort_type = self.sort_files.clone();
                        let path_clone = path.clone();
                        self.tokio_rt.spawn(async move {
                            match comic_loader.list_directory_paths(&path_clone, &sort_type).await {
                                Ok(paths) => {
                                    let dir = content::Directory { path: path_clone, files: paths };
                                    // このアクションに対応する特定のメッセージを送信します。
                                    tx.send(UiUpdateMsg::DirectoryChangedFromDrop(dir)).ok();
                                }
                                Err(e) => {
                                    tx.send(UiUpdateMsg::Error(e.to_string())).ok();
                                }
                            }
                        });
                    } else {
                        // 画像ファイルの場合は直接開く
                        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                        if ImageExtension::from_str(&ext).is_some() {
                            debug!("Dropped image file: {:?}", path);
                            self.open_new_file(path.clone());
                        } else if FileExtension::from_str(&ext).is_some_and(|ext| ext == FileExtension::Zip) {
                            // ZIPファイルは直接開く
                            debug!("Dropped zip file: {:?}", path);
                            self.open_new_file(path.clone());
                        } else {
                            debug!("Dropped unsupported file: {:?}", path);
                        }
                    }
                }
            }
        }
    }

    /// 画像の切り替えを行います。
    fn handle_image_navigation(&mut self, ctx: &egui::Context){
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight) || i.raw_scroll_delta.y < 0.0) {
            self.show_next_content();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft) || i.raw_scroll_delta.y > 0.0) {
            self.show_previous_content();
        }
    }

    /// ドラッグ＆ドロップによって新しいファイルをオープンする処理を行います。
    fn open_new_file(&mut self, path: PathBuf) {
        self.image_cache.lock().unwrap().clear();
        self.content_file = None;
        self.directory = None;
        self.parent_directory = None;
        self.current_image_handle = None;
        self.load_and_open_path(path, InitialPage::First);
    }
    
    /// パスからファイルを読み込んで開く処理を共通化
    fn load_and_open_path(&self, path: PathBuf, initial_page: InitialPage) {
        let comic_loader = self.comic_loader.clone();
        let tx = self.update_tx.clone();
        let sort_type = self.sort_files.clone();

        self.tokio_rt.spawn(async move {
            let metadata_result = tokio::fs::metadata(&path).await;

            let is_dir = if let Ok(metadata) = metadata_result {
                metadata.is_dir()
            } else {
                tx.send(UiUpdateMsg::Error(format!("Failed to get metadata for {:?}", path))).ok();
                return;
            };

            if is_dir {
                // ディレクトリです。内容をリストアップし、最初または最後の画像ファイルを開きます。
                match comic_loader.list_directory_paths(&path, &sort_type).await {
                    Ok(paths) => {
                        let file_to_open = match initial_page {
                            InitialPage::First => paths.iter().find(|p| ImageExtension::from_str(p.extension().and_then(|s| s.to_str()).unwrap_or("")).is_some()),
                            InitialPage::Last => paths.iter().rfind(|p| ImageExtension::from_str(p.extension().and_then(|s| s.to_str()).unwrap_or("")).is_some()),
                        };

                        if let Some(p) = file_to_open {
                            // 画像ファイルが見つかりました。読み込みます。
                            match comic_loader.load_comic_file(p.clone()).await {
                                Ok(comic_file) => {
                                    tx.send(UiUpdateMsg::ComicFileLoaded(comic_file, initial_page)).ok();
                                }
                                Err(e) => { tx.send(UiUpdateMsg::Error(e.to_string())).ok(); }
                            }
                        } else {
                            // ディレクトリ内に画像ファイルがありません。ディレクトリ自体を読み込みます。
                            let dir = content::Directory { path, files: paths };
                            tx.send(UiUpdateMsg::DirectoryLoaded(dir)).ok();
                        }
                    }
                    Err(e) => { tx.send(UiUpdateMsg::Error(e.to_string())).ok(); }
                }
            } else {
                // ファイルです。
                match comic_loader.load_comic_file(path).await {
                    Ok(comic_file) => {
                        tx.send(UiUpdateMsg::ComicFileLoaded(comic_file, initial_page)).ok();
                    }
                    Err(e) => {
                        tx.send(UiUpdateMsg::Error(e.to_string())).ok();
                    }
                }
            }
        });
    }

    /// 指定されたComicFileを開き、表示の準備をします。
    fn open_comic_file(&mut self, file: ComicFile, initial_page: InitialPage) {
        debug!("Opening comic file: {:?}", file.path);
        let path = file.path.clone();
        
        self.current_page_index = match initial_page {
            InitialPage::First => 0,
            InitialPage::Last => match &file.file_type {
                FileType::Zip(zip_file) => zip_file.entries.len().saturating_sub(1),
                _ => 0,
            }
        };
        self.content_file = Some(file);
        self.load_image_for_display();
        
        let container_path = if path.is_dir() {
            path.clone()
        } else {
            path.parent().unwrap_or(&path).to_path_buf()
        };
        
        let needs_reload = self.directory.as_ref().map_or(true, |d| d.path != container_path);

        if needs_reload {
            debug!("Directory has changed to {:?}. Reloading file list.", container_path);
            self.current_directory_path = Some(container_path.clone());
            self.load_directory_content(container_path.clone(), false);

            if let Some(parent_path) = container_path.parent() {
                self.load_directory_content(parent_path.to_path_buf(), true);
            } else {
                self.parent_directory = None;
            }
        } else {
            debug!("Staying in the same directory ({:?}). No reload needed.", container_path);
        }
    }

    /// 指定されたディレクトリの内容を非同期でロードします。
    fn load_directory_content(&self, path: PathBuf, is_parent: bool) {
        let comic_loader = self.comic_loader.clone();
        let tx = self.update_tx.clone();
        let sort_type = self.sort_files.clone();

        self.tokio_rt.spawn(async move {
            match comic_loader.list_directory_paths(&path, &sort_type).await {
                Ok(paths) => {
                    debug!("Loaded directory: {:?}", path);
                    let dir = content::Directory { path, files: paths };
                    let msg = if is_parent {
                        UiUpdateMsg::ParentDirectoryLoaded(dir)
                    } else {
                        UiUpdateMsg::DirectoryLoaded(dir)
                    };
                    tx.send(msg).ok();
                }
                Err(e) => {
                    tx.send(UiUpdateMsg::Error(e.to_string())).ok();
                }
            }
        });
    }

    /// 現在の`content_file`と`current_page_index`に基づいて画像を表示します。
    fn load_image_for_display(&mut self) {
        let file = match self.content_file.as_ref() {
            Some(f) => f.clone(),
            None => return,
        };
        let page_index = self.current_page_index;
        
        let key = match &file.file_type {
            FileType::Image(_) => CacheKey::File(file.path.clone()),
            FileType::Zip(_) => CacheKey::ZipEntry(file.path.clone(), page_index),
            _ => return,
        };

        if let Some(image_data) = self.image_cache.lock().unwrap().get(&key) {
            debug!("Cache hit for {:?}", key);
            self.decode_and_display(image_data);
        } else {
            debug!("Cache miss for {:?}. Loading from source.", key);
            self.load_from_source_and_display(file, page_index, key.clone());
        }

        self.update_cache_and_prefetch(&key);
    }
    
    /// ソースから画像を直接読み込み、表示し、キャッシュに格納する
    fn load_from_source_and_display(&self, file: ComicFile, page_index: usize, key: CacheKey) {
        let comic_loader = self.comic_loader.clone();
        let tx = self.update_tx.clone();
        let image_cache = self.image_cache.clone();

        self.tokio_rt.spawn(async move {
            let image_data_result = match &file.file_type {
                FileType::Image(_) => tokio::fs::read(&file.path).await.map_err(|e| e.into()),
                FileType::Zip(zip_file) => {
                    match zip_file.entries.get(page_index) {
                        Some(entry) => comic_loader.load_image_from_zip(&zip_file.path, entry).await.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string())),
                        None => Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Page not found in zip")),
                    }
                }
                _ => unreachable!(),
            };

            match image_data_result {
                Ok(data) => {
                    image_cache.lock().unwrap().insert_prefetched_data(key, data.clone());
                    if let Ok(img) = image::load_from_memory(&data) {
                        let color_image = egui::ColorImage::from_rgba_unmultiplied(
                            [img.width() as _, img.height() as _],
                            img.to_rgba8().as_flat_samples().as_slice(),
                        );
                        tx.send(UiUpdateMsg::ImageLoaded(color_image)).ok();
                    } else {
                        tx.send(UiUpdateMsg::Error("Failed to decode image".to_string())).ok();
                    }
                }
                Err(e) => {
                    tx.send(UiUpdateMsg::Error(e.to_string())).ok();
                }
            }
        });
    }

    /// バイトデータから画像をデコードしてUIに表示する
    fn decode_and_display(&self, image_data: Vec<u8>) {
        if let Ok(img) = image::load_from_memory(&image_data) {
            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [img.width() as _, img.height() as _],
                img.to_rgba8().as_flat_samples().as_slice(),
            );
            self.update_tx.send(UiUpdateMsg::ImageLoaded(color_image)).ok();
        } else {
            self.update_tx.send(UiUpdateMsg::Error("Failed to decode cached image".to_string())).ok();
        }
    }
    
    /// キャッシュウィンドウを更新し、必要なプリフェッチタスクを開始する
    fn update_cache_and_prefetch(&mut self, center_key: &CacheKey) {
        let Some(all_keys) = (|| -> Option<Vec<CacheKey>> {
            let file = self.content_file.as_ref()?;
            match &file.file_type {
                FileType::Image(_) => {
                    let dir = self.directory.as_ref()?;
                    Some(dir.files.iter().map(|p| CacheKey::File(p.clone())).collect())
                },
                FileType::Zip(zip_file) => {
                    Some((0..zip_file.entries.len()).map(|i| CacheKey::ZipEntry(file.path.clone(), i)).collect())
                },
                _ => None,
            }
        })() else { return; };
        
        let keys_to_prefetch = self.image_cache.lock().unwrap().update_window(center_key, &all_keys);

        if !keys_to_prefetch.is_empty() {
            debug!("Prefetching {} keys.", keys_to_prefetch.len());
            for key in keys_to_prefetch {
                let comic_loader = self.comic_loader.clone();
                let image_cache = self.image_cache.clone();
                self.tokio_rt.spawn(async move {
                    let data_result = match &key {
                        CacheKey::File(path) => tokio::fs::read(path).await.map_err(|e| e.to_string()),
                        CacheKey::ZipEntry(path, index) => {
                            // ZIPのプリフェッチは複雑なため、一旦実装を省略します。
                            // ここを実装するには、非同期タスク内でComicFileを再ロードする必要がある
                             return;
                        }
                    };
                    if let Ok(data) = data_result {
                        image_cache.lock().unwrap().insert_prefetched_data(key, data);
                    }
                });
            }
        }
    }

    /// 次のコンテンツ（画像/ページ/コンテナ）を表示します。
    pub fn show_next_content(&mut self) {
        let (file, dir) = match (self.content_file.as_ref(), self.directory.as_ref()) {
            (Some(f), Some(d)) => (f, d),
            _ => return,
        };

        if let FileType::Zip(zip_file) = &file.file_type {
            if self.current_page_index + 1 < zip_file.entries.len() {
                self.current_page_index += 1;
                debug!("Next page in zip: {}", self.current_page_index);
                self.load_image_for_display();
                return;
            }
        }

        // 2. 現在のファイルが属するディレクトリ内の次のファイルに移動します。
        //    - `dir.files.iter().position(|p| p == &file.path)` で現在のファイルのインデックスを取得します。
        //    - 次のファイルが存在する場合 (`dir.files.get(current_idx + 1)`)、
        //      そのパスを `load_and_open_path` に渡し、最初のページから開きます。
        if let Some(current_idx) = dir.files.iter().position(|p| p == &file.path) {
            if let Some(next_path) = dir.files.get(current_idx + 1) {
                self.load_and_open_path(next_path.clone(), InitialPage::First);
                return;
            }
        }
        // 3. 現在のディレクトリの次のコンテナ（親ディレクトリ内の次のディレクトリまたはZIPファイル）に移動します。
        //    - 上記の条件が満たされない場合、`move_to_container(true)` を呼び出して次のコンテナに移動します。
        self.move_to_container(true);
    }

    /// 前のコンテンツ（画像/ページ/コンテナ）を表示します。
    ///
    /// この関数は、現在の表示状態に基づいて、前の画像、ZIPファイル内の前のページ、
    /// または親ディレクトリ内の前のコンテナ（ディレクトリ/ZIPファイル）に移動します。
    ///
    /// # 動作
    /// 1. `self.content_file` と `self.directory` が存在しない場合は処理を終了します。
    /// 2. 現在のファイルがZIPファイルの場合、前のページに移動します。
    ///    - `self.current_page_index` が0より大きい場合、インデックスをデクリメントし、
    ///      `load_image_for_display` を呼び出して新しいページを表示します。
    /// 3. 現在のファイルが属するディレクトリ内の前のファイルに移動します。
    ///    - `dir.files.iter().position(|p| p == &file.path)` で現在のファイルのインデックスを取得します。
    ///    - 前のファイルが存在する場合 (`current_idx > 0` かつ `dir.files.get(current_idx - 1)`)、
    ///      そのパスを `load_and_open_path` に渡し、最後のページから開きます。
    /// 4. 現在のディレクトリの前のコンテナ（親ディレクトリ内の前のディレクトリまたはZIPファイル）に移動します。
    ///    - 上記の条件が満たされない場合、`move_to_container(false)` を呼び出して前のコンテナに移動します。
    pub fn show_previous_content(&mut self) {
        let (file, dir) = match (self.content_file.as_ref(), self.directory.as_ref()) {
            (Some(f), Some(d)) => (f, d),
            _ => return,
        };

        if let FileType::Zip(_) = &file.file_type {
            if self.current_page_index > 0 {
                self.current_page_index -= 1;
                debug!("Previous page in zip: {}", self.current_page_index);
                self.load_image_for_display();
                return;
            }
        }

        if let Some(current_idx) = dir.files.iter().position(|p| p == &file.path) {
            if current_idx > 0 {
                if let Some(prev_path) = dir.files.get(current_idx - 1) {
                    self.load_and_open_path(prev_path.clone(), InitialPage::Last);
                    return;
                }
            }
        }
        self.move_to_container(false);
    }
    
    /// 次または前のコンテナ（ディレクトリ/ZIP）に移動します。
    ///
    /// この関数は、現在のディレクトリが属する親ディレクトリ内のファイルリストを検索し、
    /// 指定された方向（次または前）に応じて、次のまたは前のコンテナファイルを開きます。
    ///
    /// # 引数
    /// - `next`: `true` の場合、次のコンテナに移動します。`false` の場合、前のコンテナに移動します。
    ///
    /// # 動作
    /// 1. `self.parent_directory` と `self.current_directory_path` が存在しない場合は処理を終了します。
    /// 2. `parent_dir.files` 内で `current_dir_path` のインデックスを検索します。
    /// 3. インデックスが見つかった場合：
    ///    - `next` の値に基づいて、ターゲットとなるインデックス (`target_idx`) を計算します。
    ///      - `next` が `true`: `current_idx + 1`
    ///      - `next` が `false`: `current_idx.saturating_sub(1)` (アンダーフロー防止)
    ///    - `parent_dir.files.get(target_idx)` でターゲットパスを取得します。
    ///    - ターゲットパスが存在する場合、`load_and_open_path` を呼び出してそのコンテナを開きます。
    ///      - `next` が `true` の場合、`InitialPage::First` から開きます。
    ///      - `next` が `false` の場合、`InitialPage::Last` から開きます。
    fn move_to_container(&mut self, next: bool) {
        debug!("Moving to {} container,{}", if next { "next" } else { "previous" },self.current_directory_path.as_ref().map(|p| format!(" current dir: {:?}", p)).unwrap_or_default());
        let (parent_dir, current_dir_path) = match (self.parent_directory.as_ref(), self.current_directory_path.as_ref()) {
            (Some(pd), Some(cdp)) => (pd, cdp),
            _ => { return; }
        };

        if let Some(current_idx) = parent_dir.files.iter().position(|p| p == current_dir_path) {
            let target_idx = if next { current_idx + 1 } else { current_idx.saturating_sub(1) };
            if let Some(target_path) = parent_dir.files.get(target_idx) {
                let initial_page = if next { InitialPage::First } else { InitialPage::Last };
                self.load_and_open_path(target_path.clone(), initial_page);
            }
        }
    }
}