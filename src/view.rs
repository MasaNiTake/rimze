use eframe::egui::{self, Context};
use std::path::PathBuf;
use crate::content::{FileType, SortType};
use crate::ComicViewerAppState;

/// UIからアプリケーションのメインロジックへ送られるコマンドを定義します。
pub enum UiCommand {
    OpenFile(PathBuf),
    OpenFileDialog,
    CloseFile,
    SetSort(SortType),
    ChangePage(usize),
    SetMaxMemory(usize),
}

pub struct ComicViewerUI {
    pub last_open_dir: Option<PathBuf>,
}

impl ComicViewerUI {
    /// 新しい`ComicViewerUI`インスタンスを作成します。
    pub fn new() -> Self {
        Self {
            last_open_dir: directories::UserDirs::new().and_then(|ud| ud.picture_dir().map(|p| p.to_path_buf())),
        }
    }

    /// アプリケーションのメインUIを構築します。
    pub fn build_ui(&mut self, ctx: &Context, _frame: &mut eframe::Frame, app_state: &mut ComicViewerAppState) -> Vec<UiCommand> {
        let mut commands = Vec::new();

        commands.extend(self.top_panel(ctx, app_state));
        commands.extend(self.side_panel(ctx, app_state));
        commands.extend(self.bottom_panel(ctx, app_state));
        self.central_panel(ctx, app_state);

        commands
    }

    /// アプリケーションの上部パネル（メニューバー）を構築します。
    fn top_panel(&mut self, ctx: &Context, app_state: &mut ComicViewerAppState) -> Vec<UiCommand> {
        let mut commands = Vec::new();
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("ファイル", |ui| {
                    if ui.button("開く").clicked() {
                        commands.push(UiCommand::OpenFileDialog);
                        ui.close_menu();
                    }
                    if ui.button("閉じる").clicked() {
                        commands.push(UiCommand::CloseFile);
                        ui.close_menu();
                    }
                });
                ui.menu_button("設定", |ui| {
                    ui.menu_button("ソート順", |ui| {
                        if ui.radio_value(app_state.sort_files, SortType::FileName, "ファイル名").clicked() {
                            commands.push(UiCommand::SetSort(SortType::FileName));
                            ui.close_menu();
                        }
                        if ui.radio_value(app_state.sort_files, SortType::ModifiedDate, "更新日時").clicked() {
                            commands.push(UiCommand::SetSort(SortType::ModifiedDate));
                            ui.close_menu();
                        }
                        if ui.radio_value(app_state.sort_files, SortType::CreationDate, "作成日時").clicked() {
                             commands.push(UiCommand::SetSort(SortType::CreationDate));
                            ui.close_menu();
                        }
                    });
                    
                    let mut max_mem_mb = *app_state.max_load_use_memory / (1024 * 1024);
                    let slider = egui::Slider::new(&mut max_mem_mb, 10..=1000).text("最大キャッシュ (MB)");
                    if ui.add(slider).changed() {
                        commands.push(UiCommand::SetMaxMemory(max_mem_mb * 1024 * 1024));
                    }
                });
            });
        });
        commands
    }

    /// アプリケーションのサイドパネル（漫画ファイルリスト）を構築します。
    fn side_panel(&mut self, ctx: &Context, app_state: &mut ComicViewerAppState) -> Vec<UiCommand> {
        let mut commands = Vec::new();
        egui::SidePanel::left("side_panel").show(ctx, |ui| {
            ui.heading("ファイル一覧");
            egui::ScrollArea::vertical().show(ui, |ui| {
                if let Some(directory) = &app_state.directory {
                    for path in &directory.files {
                        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                        let is_selected = app_state.content_file.as_ref().map_or(false, |cf| cf.path == *path);
                        if ui.selectable_label(is_selected, file_name).clicked() {
                            commands.push(UiCommand::OpenFile(path.clone()));
                        }
                    }
                } else {
                    ui.label("ディレクトリが選択されていません。");
                }
            });
        });
        commands
    }

    /// アプリケーションの中央パネル（画像表示領域）を構築します。
    fn central_panel(&mut self, ctx: &Context, app_state: &mut ComicViewerAppState) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(image_handle) = &app_state.current_image_handle {
                let image_widget = egui::Image::new(image_handle)
                    .bg_fill(ui.style().visuals.panel_fill)
                    .max_size(ui.available_size());

                ui.centered_and_justified(|ui| {
                    ui.add(image_widget);
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("画像をドラッグ＆ドロップするか、ファイルメニューから開いてください。");
                });
            }
        });
    }

    /// アプリケーションの下部パネル（ページスライダー）を構築します。
    fn bottom_panel(&mut self, ctx: &Context, app_state: &mut ComicViewerAppState) -> Vec<UiCommand> {
        let mut commands = Vec::new();
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let (current_file_label, current_page, max_pages) = if let Some(file) = &app_state.content_file {
                    let name = file.path.file_name().unwrap_or_default().to_string_lossy();
                    let (current, total) = match &file.file_type {
                        FileType::Zip(zip_file) if !zip_file.entries.is_empty() => {
                            (*app_state.current_page_index, zip_file.entries.len())
                        },
                        _ => (0, 1),
                    };
                    (name.to_string(), current, total)
                } else {
                    ("ファイルが開かれていません".to_string(), 0, 1)
                };

                ui.label(current_file_label);
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|{
                    ui.label(format!("{}/{}", current_page + 1, max_pages));

                    let mut page_slider_index = *app_state.current_page_index;
                    let slider = egui::Slider::new(&mut page_slider_index, 0..=max_pages.saturating_sub(1))
                        .text("ページ")
                        .show_value(false);

                    if ui.add_enabled(max_pages > 1, slider).changed() {
                        commands.push(UiCommand::ChangePage(page_slider_index));
                    }
                });
            });
        });
        commands
    }
}