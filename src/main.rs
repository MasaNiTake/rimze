use eframe::egui;
use std::{path::PathBuf, sync::Arc};
use tokio::runtime;
use tokio::sync::mpsc;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

mod content;
mod settings;
mod thumbnail;
mod view;

use content::{
    CacheKey, ComicFile, Directory, FileExtension, FileType, ImageExtension, SortOrder, SortType,
};
use view::UiCommand;

/// アプリケーションのエントリーポイント。
/// Eguiアプリケーションを初期化し、実行します。
fn main() -> Result<(), eframe::Error> {
    // RUST_LOG 環境変数でログレベルを調整可能にする。
    // 未設定時のデフォルトは INFO レベルとする。
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .expect("トレーシング・サブスクライバーの初期化に失敗しました");

    info!("RIMZE を起動します");

    // Eframeのネイティブオプションを設定します。
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_drag_and_drop(true),
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
    sort_order: content::SortOrder,
    directory: Option<content::Directory>,
    parent_directory: Option<content::Directory>,
    max_load_use_memory: usize,
    tokio_rt: Arc<runtime::Runtime>,
    comic_loader: Arc<content::ComicLoader>,
    // UI スレッドからは try_lock、非同期タスクからは lock().await で取得するため tokio::sync::Mutex を使用。
    image_cache: Arc<tokio::sync::Mutex<content::ImageCache>>,
    current_page_index: usize,
    current_directory_path: Option<PathBuf>,
    ui_state: view::ComicViewerUI,
    update_tx: mpsc::Sender<UiUpdateMsg>,
    update_rx: mpsc::Receiver<UiUpdateMsg>,
    last_error: Option<String>,
    is_pointer_over_central_panel: bool,
    thumbnail_worker: Arc<thumbnail::ThumbnailWorker>,
    app_settings: settings::AppSettings,
    file_filter: String,
    /// キャッシュ使用量ログの最終出力時刻。約1秒に1回の出力に間引きするために使用。
    last_cache_log: std::time::Instant,
}

// UI構築のために必要なアプリケーション状態をまとめた構造体
pub struct ComicViewerAppState<'a> {
    pub content_file: &'a mut Option<content::ComicFile>,
    pub current_image_handle: &'a mut Option<egui::TextureHandle>,
    pub sort_files: &'a mut content::SortType,
    pub sort_order: &'a mut content::SortOrder,
    pub max_load_use_memory: &'a mut usize,
    pub directory: &'a Option<content::Directory>,
    pub current_page_index: &'a mut usize,
    pub is_pointer_over_central_panel: &'a mut bool,
    pub file_filter: &'a mut String,
    pub language: &'a mut settings::Language,
}

/// UIの更新メッセージを定義します。
pub enum UiUpdateMsg {
    ComicFileLoaded(content::ComicFile, InitialPage),
    DirectoryLoaded(content::Directory),
    ParentDirectoryLoaded(content::Directory),
    ImageLoaded(egui::ColorImage),
    DirectoryChanged(content::Directory),
    DirectoryChangedFromDrop(content::Directory),
    /// ファイルダイアログで選択されたファイルを開く。
    FilePicked(PathBuf),
    Error(String),
}

/// 読み込み後の初期ページ指定
#[derive(Clone, Copy)]
pub enum InitialPage {
    First,
    Last,
}

/// UI 更新チャネルのバッファ容量。
///
/// UI↔非同期タスク間通信用の `tokio::sync::mpsc` チャネルの容量。
/// UI スレッドは毎フレーム `try_recv` で受信するため、十分な余裕を持たせる。
const UI_UPDATE_CHANNEL_CAPACITY: usize = 256;

impl eframe::App for MyApp {
    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // eframe 0.34+ requires the `ui` method to be implemented.
        // However, we completely override the `update` method, which is the main entry point called by the eframe runner,
        // so this dummy `ui` method is never actually executed.
    }

    /// アプリケーションのUIを更新します。
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let mut app_state = ComicViewerAppState {
            content_file: &mut self.content_file,
            current_image_handle: &mut self.current_image_handle,
            sort_files: &mut self.sort_files,
            sort_order: &mut self.sort_order,
            max_load_use_memory: &mut self.max_load_use_memory,
            directory: &self.directory,
            current_page_index: &mut self.current_page_index,
            is_pointer_over_central_panel: &mut self.is_pointer_over_central_panel,
            file_filter: &mut self.file_filter,
            language: &mut self.app_settings.language,
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
                            let ext = path
                                .extension()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            ImageExtension::from_str(&ext).is_some()
                        }) {
                            debug!("Auto-opening first file in new directory: {:?}", first_file);
                            self.open_new_file(first_file.clone());
                        } else {
                            debug!("No image or zip files found in directory");
                        }
                    }
                }
                UiUpdateMsg::FilePicked(path) => {
                    debug!("File picked: {:?}", path);
                    // 選択されたファイルの親ディレクトリを次回の初期表示先として記憶
                    self.ui_state.last_open_dir = path.parent().map(|p| p.to_path_buf());
                    // last_open_dir の更新を設定ファイルへ永続化
                    self.update_and_save_settings();
                    // ファイルを開く（同期 OpenFileDialog 時の処理順序を再現）
                    self.open_new_file(path);
                }
                UiUpdateMsg::Error(err_msg) => {
                    eprintln!("Error: {}", err_msg);
                    self.last_error = Some(err_msg);
                }
            }
        }
        // 約1秒に1回、キャッシュ使用量を debug ログ出力する。
        // 更新ループは高頻度で呼ばれるため、負荷を抑えるために間引きを行う。
        // ※プロセス全体の RSS は `top` コマンド等で確認可能なため、ここでは出力しない。
        if self.last_cache_log.elapsed() >= std::time::Duration::from_secs(1) {
            self.last_cache_log = std::time::Instant::now();
            // UI スレッド（同期コンテキスト）から呼ばれるため try_lock を使用。
            // ロック取得失敗時はそのフレームのログ出力をスキップする（表示には影響しない）。
            if let Ok(cache) = self.image_cache.try_lock() {
                let cache_mb = cache.current_memory_usage() / 1024 / 1024;
                debug!("Cache usage: {} MB", cache_mb);
            }
        }

        if let Some(error) = &self.last_error {
            egui::Area::new("error_toast".into())
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -20.0))
                .show(ctx, |ui| {
                    let frame = egui::Frame::popup(ui.style());
                    frame.show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(error).color(ui.style().visuals.error_fg_color),
                        );
                    });
                });
        }
    }
}

/// バックグラウンドタスクを起動し、panic 時にログ出力する監視付き spawn。
///
/// `tokio_rt.spawn(task)` の戻り値の [`tokio::task::JoinHandle`] を監視し、
/// タスクが panic して join エラーになった場合に `tracing::error!` で記録します。
fn spawn_tracked<F>(tokio_rt: &tokio::runtime::Runtime, task: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let handle = tokio_rt.spawn(task);
    tokio_rt.spawn(async move {
        if let Err(join_err) = handle.await {
            tracing::error!("バックグラウンドタスクがパニックしました: {join_err}");
        }
    });
}

impl MyApp {
    fn update_and_save_settings(&mut self) {
        self.app_settings.sort_files = self.sort_files.clone();
        self.app_settings.sort_order = self.sort_order.clone();
        self.app_settings.max_load_use_memory = self.max_load_use_memory;
        self.app_settings.last_open_dir = self.ui_state.last_open_dir.clone();
        self.app_settings.save();
    }
}

impl MyApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (update_tx, update_rx) = mpsc::channel(UI_UPDATE_CHANNEL_CAPACITY);

        egui_extras::install_image_loaders(&cc.egui_ctx);

        let app_settings = settings::AppSettings::load();

        let mut fonts = egui::FontDefinitions::default();
        let font_filename = app_settings
            .font_name
            .as_deref()
            .unwrap_or("PlemolJPConsoleNF-Regular.ttf");

        // 検索するパスのリスト
        let mut potential_font_paths = vec![
            std::path::PathBuf::from("fonts").join(font_filename), // CWD/fonts/
        ];

        // 実行ファイルがあるディレクトリの fonts/ フォルダも確認
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                potential_font_paths.push(exe_dir.join("fonts").join(font_filename));
            }
        }

        // 設定ディレクトリの fonts/ フォルダも確認
        if let Some(config_dir) = settings::AppSettings::get_config_dir() {
            potential_font_paths.push(config_dir.join("fonts").join(font_filename));
        }

        let mut font_loaded = false;
        for path in potential_font_paths {
            if path.exists() {
                if let Ok(font_data) = std::fs::read(&path) {
                    debug!("Found font at {:?}", path);
                    fonts.font_data.insert(
                        "ja_font".to_owned(),
                        Arc::new(egui::FontData::from_owned(font_data)),
                    );

                    // `egui::FontDefinitions::default()` は Proportional/Monospace の両キーを
                    // 必ず保持するが、安全のため `if let Some` で存在確認してから挿入する。
                    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional)
                    {
                        family.insert(0, "ja_font".to_owned());
                    }
                    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                        family.push("ja_font".to_owned());
                    }
                    font_loaded = true;
                    break;
                }
            }
        }

        if font_loaded {
            cc.egui_ctx.set_fonts(fonts);
        } else {
            debug!("Japanese font (PlemolJP) not found. Using default fonts.");
        }

        // MyApp::new は Result を返さないため ? 伝播できず、起動時1回の致命的失敗として
        // 理由明記 expect で構築する（Agents.md: "Minimize unwrap/expect" の例外運用）。
        let tokio_rt = Arc::new(
            runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Tokioマルチスレッドランタイムの構築に失敗しました"),
        );
        let max_memory_usage = app_settings.max_load_use_memory;
        let image_cache = Arc::new(tokio::sync::Mutex::new(content::ImageCache::new(
            max_memory_usage,
        )));
        let comic_loader = Arc::new(content::ComicLoader::new(
            tokio_rt.clone(),
            image_cache.clone(),
        ));

        Self {
            dropped_files: Default::default(),
            content_file: None,
            current_image_handle: None,
            sort_files: app_settings.sort_files.clone(),
            sort_order: app_settings.sort_order.clone(),
            directory: None,
            parent_directory: None,
            max_load_use_memory: max_memory_usage,
            tokio_rt: tokio_rt.clone(),
            comic_loader,
            image_cache,
            current_page_index: 0,
            current_directory_path: None,
            ui_state: view::ComicViewerUI::new(app_settings.last_open_dir.clone()),
            update_tx,
            update_rx,
            last_error: None,
            is_pointer_over_central_panel: false,
            thumbnail_worker: Arc::new(thumbnail::ThumbnailWorker::spawn(&tokio_rt)),
            app_settings,
            file_filter: String::new(),
            last_cache_log: std::time::Instant::now(),
        }
    }

    /// UIから発行されたコマンドを処理します。
    fn handle_ui_command(&mut self, command: UiCommand) {
        match command {
            UiCommand::OpenFileDialog => {
                // ダイアログ構築に必要な情報を事前に取得（spawn クロージャは self にアクセス不可）
                let tx = self.update_tx.clone();
                let initial_dir = self
                    .ui_state
                    .last_open_dir
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("/"));
                // 非同期タスク内で UI スレッドをブロックせずダイアログを表示する。
                // 選択結果は UiUpdateMsg::FilePicked メッセージで UI スレッドに通知する。
                let extensions: Vec<&'static str> =
                    FileExtension::as_slice().iter().map(|ext| ext.as_str()).collect();
                spawn_tracked(&self.tokio_rt, async move {
                    let file_handle = rfd::AsyncFileDialog::new()
                        .add_filter("Image Files", &extensions)
                        .set_directory(&initial_dir)
                        .pick_file()
                        .await;
                    if let Some(handle) = file_handle {
                        let path: PathBuf = handle.path().to_path_buf();
                        let _ = tx.try_send(UiUpdateMsg::FilePicked(path));
                    }
                });
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
            UiCommand::SetSortAndOrder(sort_type, sort_order) => {
                self.sort_files = sort_type;
                self.sort_order = sort_order;
                self.update_and_save_settings();
                if let Some(path) = self.current_directory_path.clone() {
                    self.load_directory_content(path, false);
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
                // UI スレッドから呼ばれるため try_lock を使用。
                // ロック取得失敗時はデバッグログを出力し、次フレーム以降で再試行の余地を残す。
                match self.image_cache.try_lock() {
                    Ok(mut cache) => cache.set_max_memory_usage(bytes),
                    Err(e) => debug!(
                        "image_cache の try_lock に失敗したため set_max_memory_usage をスキップします: {}",
                        e
                    ),
                }
                self.update_and_save_settings();
            }
            UiCommand::SetLanguage(lang) => {
                self.app_settings.language = lang;
                self.update_and_save_settings();
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
            let painter =
                ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("file_drop_target")));
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
                        let sort_order = self.sort_order.clone();
                        let path_clone = path.clone();
                        spawn_tracked(&self.tokio_rt, async move {
                            match comic_loader
                                .list_directory_paths(&path_clone, &sort_type, &sort_order)
                                .await
                            {
                                Ok(paths) => {
                                    let dir = content::Directory {
                                        path: path_clone,
                                        files: paths,
                                    };
                                    // このアクションに対応する特定のメッセージを送信します。
                                    let _ = tx.try_send(UiUpdateMsg::DirectoryChangedFromDrop(dir));
                                }
                                Err(e) => {
                                    let _ = tx.try_send(UiUpdateMsg::Error(e.to_string()));
                                }
                            }
                        });
                    } else {
                        // 画像ファイルの場合は直接開く
                        let ext = path
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        if ImageExtension::from_str(&ext).is_some() {
                            debug!("Dropped image file: {:?}", path);
                            self.open_new_file(path.clone());
                        } else if FileExtension::from_str(&ext)
                            .is_some_and(|ext| ext == FileExtension::Zip)
                        {
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

    fn handle_image_navigation(&mut self, ctx: &egui::Context) {
        let scroll_delta: f32 = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| {
                    if let egui::Event::MouseWheel { delta, .. } = e {
                        Some(delta.y)
                    } else {
                        None
                    }
                })
                .sum()
        });
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight))
            || (self.is_pointer_over_central_panel && scroll_delta < 0.0)
        {
            self.show_next_content();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft))
            || (self.is_pointer_over_central_panel && scroll_delta > 0.0)
        {
            self.show_previous_content();
        }
    }

    /// ドラッグ＆ドロップによって新しいファイルをオープンする処理を行います。
    fn open_new_file(&mut self, path: PathBuf) {
        // UI スレッドから呼ばれるため try_lock を使用。
        // ロック取得失敗時はスキップ（新しいファイル読込時にキャッシュが再構築されるため実害は限定的）。
        if let Ok(mut cache) = self.image_cache.try_lock() {
            cache.clear();
        }
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
        let sort_order = self.sort_order.clone();

        spawn_tracked(&self.tokio_rt, async move {
            let metadata_result = tokio::fs::metadata(&path).await;

            let is_dir = if let Ok(metadata) = metadata_result {
                metadata.is_dir()
            } else {
                let _ = tx.try_send(UiUpdateMsg::Error(format!(
                    "Failed to get metadata for {:?}",
                    path
                )));
                return;
            };

            if is_dir {
                // ディレクトリです。内容をリストアップし、最初または最後の画像ファイルを開きます。
                match comic_loader
                    .list_directory_paths(&path, &sort_type, &sort_order)
                    .await
                {
                    Ok(paths) => {
                        let file_to_open = match initial_page {
                            InitialPage::First => paths.iter().find(|p| {
                                ImageExtension::from_str(
                                    p.extension().and_then(|s| s.to_str()).unwrap_or(""),
                                )
                                .is_some()
                            }),
                            InitialPage::Last => paths.iter().rfind(|p| {
                                ImageExtension::from_str(
                                    p.extension().and_then(|s| s.to_str()).unwrap_or(""),
                                )
                                .is_some()
                            }),
                        };

                        if let Some(p) = file_to_open {
                            // 画像ファイルが見つかりました。読み込みます。
                            match comic_loader.load_comic_file(p.clone()).await {
                                Ok(comic_file) => {
                                    let _ = tx.try_send(UiUpdateMsg::ComicFileLoaded(
                                        comic_file,
                                        initial_page,
                                    ));
                                }
                                Err(e) => {
                                    let _ = tx.try_send(UiUpdateMsg::Error(e.to_string()));
                                }
                            }
                        } else {
                            // ディレクトリ内に画像ファイルがありません。ディレクトリ自体を読み込みます。
                            let dir = content::Directory { path, files: paths };
                            let _ = tx.try_send(UiUpdateMsg::DirectoryLoaded(dir));
                        }
                    }
                    Err(e) => {
                        let _ = tx.try_send(UiUpdateMsg::Error(e.to_string()));
                    }
                }
            } else {
                // ファイルです。
                match comic_loader.load_comic_file(path).await {
                    Ok(comic_file) => {
                        let _ =
                            tx.try_send(UiUpdateMsg::ComicFileLoaded(comic_file, initial_page));
                    }
                    Err(e) => {
                        let _ = tx.try_send(UiUpdateMsg::Error(e.to_string()));
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
            },
        };
        self.content_file = Some(file);
        self.load_image_for_display();

        let container_path = if path.is_dir() {
            path.clone()
        } else {
            path.parent().unwrap_or(&path).to_path_buf()
        };

        let needs_reload = self
            .directory
            .as_ref()
            .map_or(true, |d| d.path != container_path);

        if needs_reload {
            debug!(
                "Directory has changed to {:?}. Reloading file list.",
                container_path
            );
            self.current_directory_path = Some(container_path.clone());
            self.load_directory_content(container_path.clone(), false);

            if let Some(parent_path) = container_path.parent() {
                self.load_directory_content(parent_path.to_path_buf(), true);
            } else {
                self.parent_directory = None;
            }
        } else {
            debug!(
                "Staying in the same directory ({:?}). No reload needed.",
                container_path
            );
            if let Some(dir) = &self.directory {
                if let Some(idx) = dir.files.iter().position(|p| p == &path) {
                    self.thumbnail_worker.set_focus(idx);
                }
            }
        }
    }

    /// 指定されたディレクトリの内容を非同期でロードします。
    fn load_directory_content(&self, path: PathBuf, is_parent: bool) {
        let comic_loader = self.comic_loader.clone();
        let tx = self.update_tx.clone();
        let sort_type = self.sort_files.clone();
        let sort_order = self.sort_order.clone();

        let thumbnail_worker = self.thumbnail_worker.clone();
        let path_clone_outer = path.clone();

        spawn_tracked(&self.tokio_rt, async move {
            match comic_loader
                .list_directory_paths(&path_clone_outer, &sort_type, &sort_order)
                .await
            {
                Ok(paths) => {
                    debug!("Loaded directory: {:?}", path_clone_outer);

                    // 親ディレクトリでない場合、サムネイル生成を開始します
                    if !is_parent {
                        thumbnail_worker.new_list(paths.clone());
                    }

                    let dir = content::Directory {
                        path: path_clone_outer,
                        files: paths,
                    };
                    let msg = if is_parent {
                        UiUpdateMsg::ParentDirectoryLoaded(dir)
                    } else {
                        UiUpdateMsg::DirectoryLoaded(dir)
                    };
                    let _ = tx.try_send(msg);
                }
                Err(e) => {
                    let _ = tx.try_send(UiUpdateMsg::Error(e.to_string()));
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

        // UI スレッド（同期コンテキスト）から呼ばれるため try_lock を使用。
        // ロック取得失敗時は「キャッシュミス扱いでソースから読み込む」に倒し、表示遅延を防ぐ。
        let cached = self
            .image_cache
            .try_lock()
            .ok()
            .and_then(|mut cache| cache.get(&key));

        if let Some(image_data) = cached {
            debug!("Cache hit for {:?}", key);
            // `image_data` は `Arc<Vec<u8>>`。デコードは `&[u8]` で借用し、フルコピーは発生しない。
            self.decode_and_display(image_data.as_slice());
            // サムネイル生成タスクへは `Arc` をムーブ（参照カウント増加以外のコストなし）。
            self.try_trigger_thumbnail(file, page_index, image_data);
        } else {
            debug!("Cache miss for {:?}. Loading from source.", key);
            self.load_from_source_and_display(file, page_index, key.clone());
        }

        self.update_cache_and_prefetch(&key);
    }

    /// サムネイル生成をバックグラウンドで試行します。
    ///
    /// `image_data` は `Arc<Vec<u8>>` を受け取り、spawn タスクへそのままムーブする。
    /// `&[u8]` ではなく `Arc` で受け渡すことで、UI スレッドでのフルコピーを回避する
    /// （spawn タスクは `'static` な所有権データを必要とするため）。
    fn try_trigger_thumbnail(&self, file: ComicFile, page_index: usize, image_data: Arc<Vec<u8>>) {
        // ZIPの場合は最初のページのみ、画像の場合はその画像のみ
        let should_generate = match &file.file_type {
            FileType::Image(_) => true,
            FileType::Zip(_) => page_index == 0,
            _ => false,
        };

        if should_generate {
            let path = file.path.clone();
            spawn_tracked(&self.tokio_rt, async move {
                // ensure_thumbnail が Vec<u8> を取る現仕様のため、ワーカースレッド側で Vec 化する
                // （UI スレッドではないため、ここでのフルコピーは許容される）。
                thumbnail::ThumbnailManager::ensure_thumbnail(path, image_data.as_slice().to_vec());
            });
        }
    }

    /// ソースから画像を直接読み込み、表示し、キャッシュに格納する
    fn load_from_source_and_display(&self, file: ComicFile, page_index: usize, key: CacheKey) {
        let comic_loader = self.comic_loader.clone();
        let tx = self.update_tx.clone();
        let image_cache = self.image_cache.clone();

        spawn_tracked(&self.tokio_rt, async move {
            let image_data_result = match &file.file_type {
                FileType::Image(_) => tokio::fs::read(&file.path).await.map_err(|e| e.into()),
                FileType::Zip(zip_file) => match zip_file.entries.get(page_index) {
                    Some(entry) => comic_loader
                        .load_image_from_zip(&zip_file.path, entry)
                        .await
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string())),
                    None => Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "Page not found in zip",
                    )),
                },
                // FileType は Image/Zip 以外は上位（load_image_for_display 等）で return 済みだが、
                // 静的保証ではないため、安全側に倒して警告ログ付きで early return する。
                _ => {
                    tracing::warn!("未対応の FileType です: {:?}", file.file_type);
                    return;
                }
            };

            match image_data_result {
                Ok(data) => {
                    // Vec<u8> を Arc で包み、キャッシュ挿入とデコード/サムネイルで共有（フルコピー回避）。
                    let data = Arc::new(data);
                    // 非同期タスク内のため lock().await で取得。Arc の clone は参照カウント増加のみ。
                    image_cache
                        .lock()
                        .await
                        .insert_prefetched_data(key, data.clone());
                    if let Some(color_image) = content::decode_bytes_to_color_image(&data) {
                        let _ = tx.try_send(UiUpdateMsg::ImageLoaded(color_image));

                        // サムネイル生成をこのタスク内からでも別タスクとしてキック
                        let should_generate = match &file.file_type {
                            FileType::Image(_) => true,
                            FileType::Zip(_) => page_index == 0,
                            _ => false,
                        };
                        if should_generate {
                            let path = file.path.clone();
                            // Arc の clone（参照カウントのみ）で共有し、ワーカー側で Vec 化する。
                            let data_for_thumb = data.clone();
                            let handle = tokio::spawn(async move {
                                thumbnail::ThumbnailManager::ensure_thumbnail(
                                    path,
                                    data_for_thumb.as_slice().to_vec(),
                                );
                            });
                            tokio::spawn(async move {
                                if let Err(join_err) = handle.await {
                                    tracing::error!(
                                        "サムネイル生成タスクがパニックしました: {join_err}"
                                    );
                                }
                            });
                        }
                    } else {
                        let _ = tx
                            .try_send(UiUpdateMsg::Error("Failed to decode image".to_string()));
                    }
                }
                Err(e) => {
                    let _ = tx.try_send(UiUpdateMsg::Error(e.to_string()));
                }
            }
        });
    }

    /// バイトデータから画像をデコードしてUIに表示する
    ///
    /// `&[u8]` を受け取り、データのフルコピーなしでデコードする。
    fn decode_and_display(&self, image_data: &[u8]) {
        // UI スレッドから呼ばれるため try_send を使用（.await 不可）。
        if let Some(color_image) = content::decode_bytes_to_color_image(image_data) {
            let _ = self.update_tx.try_send(UiUpdateMsg::ImageLoaded(color_image));
        } else {
            let _ = self.update_tx.try_send(UiUpdateMsg::Error(
                "Failed to decode cached image".to_string(),
            ));
        }
    }

    /// プリフェッチ対象キーを計算し、必要なプリフェッチタスクを開始する
    fn update_cache_and_prefetch(&mut self, center_key: &CacheKey) {
        let Some(all_keys) = (|| -> Option<Vec<CacheKey>> {
            let file = self.content_file.as_ref()?;
            match &file.file_type {
                FileType::Image(_) => {
                    let dir = self.directory.as_ref()?;
                    Some(
                        dir.files
                            .iter()
                            .map(|p| CacheKey::File(p.clone()))
                            .collect(),
                    )
                }
                FileType::Zip(zip_file) => Some(
                    (0..zip_file.entries.len())
                        .map(|i| CacheKey::ZipEntry(file.path.clone(), i))
                        .collect(),
                ),
                _ => None,
            }
        })() else {
            return;
        };

        // UI スレッドから呼ばれるため try_lock を使用。
        // ロック取得失敗時はそのフレームのプリフェッチをスキップする。
        let keys_to_prefetch = match self.image_cache.try_lock() {
            Ok(cache) => cache.compute_prefetch_keys(center_key, &all_keys),
            Err(e) => {
                debug!(
                    "image_cache の try_lock に失敗したためプリフェッチキー計算をスキップします: {}",
                    e
                );
                return;
            }
        };

        if !keys_to_prefetch.is_empty() {
            debug!("Prefetching {} keys.", keys_to_prefetch.len());
            // CacheKey::ZipEntry の index → entry_name 解決用に、現在開いている ZIP の
            // エントリ名リストを取得する。spawn する非同期クロージャは 'static であり
            // self にアクセスできないため、ループ内で事前に entry_name を解決してから
            // クロージャへ move する（SingleFile 側は key 内にパスを直接持つため不要）。
            let zip_entries: Option<&Vec<String>> = self
                .content_file
                .as_ref()
                .and_then(|f| match &f.file_type {
                    FileType::Zip(zip_file) => Some(&zip_file.entries),
                    _ => None,
                });

            for key in keys_to_prefetch {
                // CacheKey::ZipEntry の index から該当エントリ名を事前解決する。
                // index が範囲外（None）の場合はクロージャ内でスキップする。
                let zip_entry_name = match &key {
                    CacheKey::ZipEntry(_, index) => {
                        zip_entries.and_then(|entries| entries.get(*index).cloned())
                    }
                    _ => None,
                };

                let comic_loader = self.comic_loader.clone();
                let image_cache = self.image_cache.clone();
                spawn_tracked(&self.tokio_rt, async move {
                    let data_result = match &key {
                        CacheKey::File(path) => {
                            tokio::fs::read(path).await.map_err(|e| e.to_string())
                        }
                        CacheKey::ZipEntry(zip_path, _) => {
                            // index が範囲外などでエントリ名が解決できない場合はスキップする。
                            let Some(entry_name) = zip_entry_name.as_ref() else {
                                return;
                            };
                            debug!("Prefetching ZIP entry: {:?}[{}]", zip_path, entry_name);
                            comic_loader
                                .load_image_from_zip(zip_path, entry_name)
                                .await
                                .map_err(|e| e.to_string())
                        }
                    };
                    if let Ok(data) = data_result {
                        // Vec<u8> を Arc で包んでキャッシュへ共有挿入（フルコピー回避）。
                        // 非同期タスク内のため lock().await で取得。
                        image_cache
                            .lock()
                            .await
                            .insert_prefetched_data(key, Arc::new(data));
                    }
                });
            }
        }
    }

    /// フィルタリングされたファイルリストを取得します。
    fn get_filtered_files(&self) -> Vec<PathBuf> {
        let Some(dir) = &self.directory else {
            return Vec::new();
        };
        if self.file_filter.is_empty() {
            return dir.files.clone();
        }
        let filter = self.file_filter.to_lowercase();
        dir.files
            .iter()
            .filter(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().to_lowercase().contains(&filter))
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    /// 次のコンテンツ（画像/ページ/コンテナ）を表示します。
    pub fn show_next_content(&mut self) {
        let file = match self.content_file.as_ref() {
            Some(f) => f,
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
        //    フィルタリングが有効な場合は、フィルタリングされたリスト内を移動します。
        let files = self.get_filtered_files();
        if let Some(current_idx) = files.iter().position(|p| p == &file.path) {
            if let Some(next_path) = files.get(current_idx + 1) {
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
        let file = match self.content_file.as_ref() {
            Some(f) => f,
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

        let files = self.get_filtered_files();
        if let Some(current_idx) = files.iter().position(|p| p == &file.path) {
            if current_idx > 0 {
                if let Some(prev_path) = files.get(current_idx - 1) {
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
        debug!(
            "Moving to {} container,{}",
            if next { "next" } else { "previous" },
            self.current_directory_path
                .as_ref()
                .map(|p| format!(" current dir: {:?}", p))
                .unwrap_or_default()
        );
        let (parent_dir, current_dir_path) = match (
            self.parent_directory.as_ref(),
            self.current_directory_path.as_ref(),
        ) {
            (Some(pd), Some(cdp)) => (pd, cdp),
            _ => {
                return;
            }
        };

        if let Some(current_idx) = parent_dir.files.iter().position(|p| p == current_dir_path) {
            let target_idx = if next {
                current_idx + 1
            } else {
                current_idx.saturating_sub(1)
            };
            if let Some(target_path) = parent_dir.files.get(target_idx) {
                let initial_page = if next {
                    InitialPage::First
                } else {
                    InitialPage::Last
                };
                self.load_and_open_path(target_path.clone(), initial_page);
            }
        }
    }
}
