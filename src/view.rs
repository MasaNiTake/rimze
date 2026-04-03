use eframe::egui::{self, Context};
use std::path::PathBuf;
use crate::content::{FileType, SortType, SortOrder};
use crate::ComicViewerAppState;
use crate::thumbnail::ThumbnailManager;

/// UIからアプリケーションのメインロジックへ送られるコマンドを定義します。
pub enum UiCommand {
    OpenFile(PathBuf),
    OpenFileDialog,
    CloseFile,
    SetSortAndOrder(SortType, SortOrder),
    ChangePage(usize),
    SetMaxMemory(usize),
}

pub struct ComicViewerUI {
    pub last_open_dir: Option<PathBuf>,
    pub last_selected_path: Option<PathBuf>,
}

impl ComicViewerUI {
    /// 新しい`ComicViewerUI`インスタンスを作成します。
    pub fn new(last_open_dir: Option<PathBuf>) -> Self {
        Self {
            last_open_dir,
            last_selected_path: None,
        }
    }

    /// アプリケーションのメインUIを構築します。
    pub fn build_ui(&mut self, ctx: &Context, _frame: &mut eframe::Frame, app_state: &mut ComicViewerAppState) -> Vec<UiCommand> {
        let mut commands = Vec::new();

        let monitor_height = ctx.input(|i| i.viewport().monitor_size.map(|s| s.y).unwrap_or(1080.0));
        let thumb_height = monitor_height * 0.05;

        commands.extend(self.top_panel(ctx, app_state));
        commands.extend(self.side_panel(ctx, app_state, thumb_height));
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
                        ui.menu_button("ファイル名", |ui| {
                            let is_asc = *app_state.sort_files == SortType::FileName && *app_state.sort_order == SortOrder::Ascending;
                            if ui.radio(is_asc, "昇順").clicked() {
                                commands.push(UiCommand::SetSortAndOrder(SortType::FileName, SortOrder::Ascending));
                                ui.close();
                            }
                            let is_desc = *app_state.sort_files == SortType::FileName && *app_state.sort_order == SortOrder::Descending;
                            if ui.radio(is_desc, "降順").clicked() {
                                commands.push(UiCommand::SetSortAndOrder(SortType::FileName, SortOrder::Descending));
                                ui.close();
                            }
                        });
                        ui.menu_button("更新日時", |ui| {
                            let is_asc = *app_state.sort_files == SortType::ModifiedDate && *app_state.sort_order == SortOrder::Ascending;
                            if ui.radio(is_asc, "昇順").clicked() {
                                commands.push(UiCommand::SetSortAndOrder(SortType::ModifiedDate, SortOrder::Ascending));
                                ui.close();
                            }
                            let is_desc = *app_state.sort_files == SortType::ModifiedDate && *app_state.sort_order == SortOrder::Descending;
                            if ui.radio(is_desc, "降順").clicked() {
                                commands.push(UiCommand::SetSortAndOrder(SortType::ModifiedDate, SortOrder::Descending));
                                ui.close();
                            }
                        });
                        ui.menu_button("作成日時", |ui| {
                            let is_asc = *app_state.sort_files == SortType::CreationDate && *app_state.sort_order == SortOrder::Ascending;
                            if ui.radio(is_asc, "昇順").clicked() {
                                commands.push(UiCommand::SetSortAndOrder(SortType::CreationDate, SortOrder::Ascending));
                                ui.close();
                            }
                            let is_desc = *app_state.sort_files == SortType::CreationDate && *app_state.sort_order == SortOrder::Descending;
                            if ui.radio(is_desc, "降順").clicked() {
                                commands.push(UiCommand::SetSortAndOrder(SortType::CreationDate, SortOrder::Descending));
                                ui.close();
                            }
                        });
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
    fn side_panel(&mut self, ctx: &Context, app_state: &mut ComicViewerAppState, thumb_height: f32) -> Vec<UiCommand> {
        let mut commands = Vec::new();
        egui::SidePanel::left("side_panel")
            .default_width(250.0)
            .show(ctx, |ui| {
                ui.heading("ファイル一覧");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.add(egui::TextEdit::singleline(app_state.file_filter)
                        .hint_text("ファイル名を検索...")
                    );
                    if ui.button("×").on_hover_text("検索をクリア").clicked() {
                        app_state.file_filter.clear();
                    }
                });
            ui.add_space(4.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                if let Some(directory) = &app_state.directory {
                    let mut scroll_to_path = None;
                    if let Some(cf) = &app_state.content_file {
                        if self.last_selected_path.as_ref() != Some(&cf.path) {
                            scroll_to_path = Some(cf.path.clone());
                            self.last_selected_path = Some(cf.path.clone());
                        }
                    } else {
                        self.last_selected_path = None;
                    }

                    let filter = app_state.file_filter.to_lowercase();
                    let filtered_files = directory.files.iter().filter(|path| {
                        if filter.is_empty() {
                            return true;
                        }
                        path.file_name()
                            .map(|n| n.to_string_lossy().to_lowercase().contains(&filter))
                            .unwrap_or(false)
                    });

                    for path in filtered_files {
                        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                        let is_selected = app_state.content_file.as_ref().map_or(false, |cf| cf.path == *path);
                        let thumb_path = ThumbnailManager::get_thumbnail_path(path);

                        let row_height = thumb_height.max(24.0);
                        let (rect, response) = ui.allocate_at_least(egui::vec2(ui.available_width(), row_height), egui::Sense::click());
                        
                        if ui.is_rect_visible(rect) {
                            let visuals = ui.style().interact_selectable(&response, is_selected);
                            if is_selected || response.hovered() {
                                ui.painter().rect_filled(rect, visuals.rounding(), visuals.bg_fill);
                            }
                            
                            let mut child_ui = ui.new_child(egui::UiBuilder::new()
                                .max_rect(rect)
                                .layout(egui::Layout::left_to_right(egui::Align::Center)));
                            child_ui.add_space(4.0);
                            
                            // サムネイル表示エリア（固定サイズでアライメントを確保）
                            let (thumb_rect, _) = child_ui.allocate_exact_size(egui::vec2(thumb_height, thumb_height), egui::Sense::hover());
                            let mut thumb_ui = child_ui.new_child(egui::UiBuilder::new()
                                .max_rect(thumb_rect)
                                .layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)));

                            if let Some(tp) = thumb_path {
                                if tp.exists() {
                                    thumb_ui.add(egui::Image::new(format!("file://{}", tp.display()))
                                        .max_size(egui::vec2(thumb_height, thumb_height))
                                        .corner_radius(2.0));
                                } else {
                                    thumb_ui.painter().rect_filled(thumb_rect, 2.0, ui.visuals().faint_bg_color);
                                }
                            }
                            child_ui.add_space(8.0);
                            child_ui.label(egui::RichText::new(file_name).color(visuals.fg_stroke.color));
                        }

                        if response.clicked() {
                            commands.push(UiCommand::OpenFile(path.clone()));
                        }
                        if Some(path) == scroll_to_path.as_ref() {
                            response.scroll_to_me(Some(egui::Align::TOP));
                        }
                    }
                } else {
                    ui.label("ディレクトリが選択されていません。");
                    self.last_selected_path = None;
                }
            });
        });
        commands
    }

    /// アプリケーションの中央パネル（画像表示領域）を構築します。
    fn central_panel(&mut self, ctx: &Context, app_state: &mut ComicViewerAppState) {
        let response = egui::CentralPanel::default().show(ctx, |ui| {
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
        }).response;
        *app_state.is_pointer_over_central_panel = response.hovered();
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