use crate::content::{FileType, SortOrder, SortType};
use crate::thumbnail::ThumbnailManager;
use crate::ComicViewerAppState;
use eframe::egui::{self, Context};
use std::path::PathBuf;

/// UIからアプリケーションのメインロジックへ送られるコマンドを定義します。
pub enum UiCommand {
    OpenFile(PathBuf),
    OpenFileDialog,
    CloseFile,
    SetSortAndOrder(SortType, SortOrder),
    ChangePage(usize),
    SetMaxMemory(usize),
    SetLanguage(crate::settings::Language),
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

    fn tr<'a>(&self, lang: &crate::settings::Language, ja: &'a str, en: &'a str) -> &'a str {
        match lang {
            crate::settings::Language::Japanese => ja,
            crate::settings::Language::English => en,
        }
    }

    /// アプリケーションのメインUIを構築します。
    pub fn build_ui(
        &mut self,
        ctx: &Context,
        _frame: &mut eframe::Frame,
        app_state: &mut ComicViewerAppState,
    ) -> Vec<UiCommand> {
        let mut commands = Vec::new();

        let monitor_height =
            ctx.input(|i| i.viewport().monitor_size.map(|s| s.y).unwrap_or(1080.0));
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
        eframe::egui::Panel::top::top("top_panel").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(self.tr(app_state.language, "ファイル", "File"), |ui| {
                    if ui
                        .button(self.tr(app_state.language, "開く", "Open"))
                        .clicked()
                    {
                        commands.push(UiCommand::OpenFileDialog);
                        ui.close();
                    }
                    if ui
                        .button(self.tr(app_state.language, "閉じる", "Close"))
                        .clicked()
                    {
                        commands.push(UiCommand::CloseFile);
                        ui.close();
                    }
                });
                ui.menu_button(self.tr(app_state.language, "設定", "Settings"), |ui| {
                    ui.menu_button(
                        self.tr(app_state.language, "ソート順", "Sort Order"),
                        |ui| {
                            ui.menu_button(
                                self.tr(app_state.language, "ファイル名", "File Name"),
                                |ui| {
                                    let is_asc = app_state.sort_files == &SortType::FileName
                                        && app_state.sort_order == &SortOrder::Ascending;
                                    if ui
                                        .radio(
                                            is_asc,
                                            self.tr(app_state.language, "昇順", "Ascending"),
                                        )
                                        .clicked()
                                    {
                                        commands.push(UiCommand::SetSortAndOrder(
                                            SortType::FileName,
                                            SortOrder::Ascending,
                                        ));
                                        ui.close();
                                    }
                                    let is_desc = app_state.sort_files == &SortType::FileName
                                        && app_state.sort_order == &SortOrder::Descending;
                                    if ui
                                        .radio(
                                            is_desc,
                                            self.tr(app_state.language, "降順", "Descending"),
                                        )
                                        .clicked()
                                    {
                                        commands.push(UiCommand::SetSortAndOrder(
                                            SortType::FileName,
                                            SortOrder::Descending,
                                        ));
                                        ui.close();
                                    }
                                },
                            );
                            ui.menu_button(
                                self.tr(app_state.language, "更新日時", "Modified Date"),
                                |ui| {
                                    let is_asc = app_state.sort_files == &SortType::ModifiedDate
                                        && app_state.sort_order == &SortOrder::Ascending;
                                    if ui
                                        .radio(
                                            is_asc,
                                            self.tr(app_state.language, "昇順", "Ascending"),
                                        )
                                        .clicked()
                                    {
                                        commands.push(UiCommand::SetSortAndOrder(
                                            SortType::ModifiedDate,
                                            SortOrder::Ascending,
                                        ));
                                        ui.close();
                                    }
                                    let is_desc = app_state.sort_files == &SortType::ModifiedDate
                                        && app_state.sort_order == &SortOrder::Descending;
                                    if ui
                                        .radio(
                                            is_desc,
                                            self.tr(app_state.language, "降順", "Descending"),
                                        )
                                        .clicked()
                                    {
                                        commands.push(UiCommand::SetSortAndOrder(
                                            SortType::ModifiedDate,
                                            SortOrder::Descending,
                                        ));
                                        ui.close();
                                    }
                                },
                            );
                            ui.menu_button(
                                self.tr(app_state.language, "作成日時", "Creation Date"),
                                |ui| {
                                    let is_asc = app_state.sort_files == &SortType::CreationDate
                                        && app_state.sort_order == &SortOrder::Ascending;
                                    if ui
                                        .radio(
                                            is_asc,
                                            self.tr(app_state.language, "昇順", "Ascending"),
                                        )
                                        .clicked()
                                    {
                                        commands.push(UiCommand::SetSortAndOrder(
                                            SortType::CreationDate,
                                            SortOrder::Ascending,
                                        ));
                                        ui.close();
                                    }
                                    let is_desc = app_state.sort_files == &SortType::CreationDate
                                        && app_state.sort_order == &SortOrder::Descending;
                                    if ui
                                        .radio(
                                            is_desc,
                                            self.tr(app_state.language, "降順", "Descending"),
                                        )
                                        .clicked()
                                    {
                                        commands.push(UiCommand::SetSortAndOrder(
                                            SortType::CreationDate,
                                            SortOrder::Descending,
                                        ));
                                        ui.close();
                                    }
                                },
                            );
                        },
                    );

                    ui.menu_button(self.tr(app_state.language, "言語", "Language"), |ui| {
                        if ui
                            .radio(
                                app_state.language == &crate::settings::Language::English,
                                "English",
                            )
                            .clicked()
                        {
                            commands
                                .push(UiCommand::SetLanguage(crate::settings::Language::English));
                            ui.close();
                        }
                        if ui
                            .radio(
                                app_state.language == &crate::settings::Language::Japanese,
                                "日本語",
                            )
                            .clicked()
                        {
                            commands
                                .push(UiCommand::SetLanguage(crate::settings::Language::Japanese));
                            ui.close();
                        }
                    });

                    let mut max_mem_mb = *app_state.max_load_use_memory / (1024 * 1024);
                    let slider = egui::Slider::new(&mut max_mem_mb, 10..=1000).text(self.tr(
                        app_state.language,
                        "最大キャッシュ (MB)",
                        "Max Cache (MB)",
                    ));
                    if ui.add(slider).changed() {
                        commands.push(UiCommand::SetMaxMemory(max_mem_mb * 1024 * 1024));
                    }
                });
            });
        });
        commands
    }

    /// アプリケーションのサイドパネル（漫画ファイルリスト）を構築します。
    fn side_panel(
        &mut self,
        ctx: &Context,
        app_state: &mut ComicViewerAppState,
        thumb_height: f32,
    ) -> Vec<UiCommand> {
        let mut commands = Vec::new();
        let screen_width = ctx.screen_rect().width();
        let side_min_width = screen_width * 0.02;
        let central_min_width = screen_width * 0.03;
        let side_max_width = screen_width - central_min_width;

        egui::SidePanel::left("side_panel")
            .default_width(250.0)
            .min_width(side_min_width)
            .max_width(side_max_width)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading(self.tr(app_state.language, "ファイル一覧", "File List"));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    // 親パネルのサイズを強制せずに、現在のパネル幅に合わせて伸縮させます。
                    ui.add(
                        egui::TextEdit::singleline(app_state.file_filter)
                            .hint_text(self.tr(
                                app_state.language,
                                "ファイル名を検索...",
                                "Search files...",
                            ))
                            .desired_width(ui.available_width() - 40.0),
                    );
                    if ui
                        .button("×")
                        .on_hover_text(self.tr(app_state.language, "検索をクリア", "Clear search"))
                        .clicked()
                    {
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
                            let is_selected = app_state
                                .content_file
                                .as_ref()
                                .map_or(false, |cf| cf.path == *path);
                            let thumb_path = ThumbnailManager::get_thumbnail_path(path);

                            let row_height = thumb_height.max(24.0);
                            let (rect, response) = ui.allocate_at_least(
                                egui::vec2(ui.available_width(), row_height),
                                egui::Sense::click(),
                            );

                            if ui.is_rect_visible(rect) {
                                let visuals =
                                    ui.style().interact_selectable(&response, is_selected);
                                if is_selected || response.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        visuals.rounding(),
                                        visuals.bg_fill,
                                    );
                                }

                                let mut child_ui = ui.new_child(
                                    egui::UiBuilder::new()
                                        .max_rect(rect)
                                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                                );
                                child_ui.add_space(4.0);

                                // サムネイル表示エリア（固定サイズでアライメントを確保）
                                let (thumb_rect, _) = child_ui.allocate_exact_size(
                                    egui::vec2(thumb_height, thumb_height),
                                    egui::Sense::hover(),
                                );
                                let mut thumb_ui = child_ui.new_child(
                                    egui::UiBuilder::new().max_rect(thumb_rect).layout(
                                        egui::Layout::centered_and_justified(
                                            egui::Direction::LeftToRight,
                                        ),
                                    ),
                                );

                                if let Some(tp) = thumb_path {
                                    if tp.exists() {
                                        thumb_ui.add(
                                            egui::Image::new(format!("file://{}", tp.display()))
                                                .max_size(egui::vec2(thumb_height, thumb_height))
                                                .corner_radius(2.0),
                                        );
                                    } else {
                                        thumb_ui.painter().rect_filled(
                                            thumb_rect,
                                            2.0,
                                            ui.visuals().faint_bg_color,
                                        );
                                    }
                                }
                                child_ui.add_space(8.0);
                                child_ui.label(
                                    egui::RichText::new(file_name).color(visuals.fg_stroke.color),
                                );
                            }

                            if response.clicked() {
                                commands.push(UiCommand::OpenFile(path.clone()));
                            }
                            if Some(path) == scroll_to_path.as_ref() {
                                response.scroll_to_me(Some(egui::Align::TOP));
                            }
                        }
                    } else {
                        ui.label(self.tr(
                            app_state.language,
                            "ディレクトリが選択されていません。",
                            "No directory selected.",
                        ));
                        self.last_selected_path = None;
                    }
                });
            });
        commands
    }

    /// アプリケーションの中央パネル（画像表示領域）を構築します。
    fn central_panel(&mut self, ctx: &Context, app_state: &mut ComicViewerAppState) {
        let response = egui::CentralPanel::default()
            .show(ctx, |ui| {
                if let Some(image_handle) = &app_state.current_image_handle {
                    let image_widget = egui::Image::new(image_handle)
                        .bg_fill(ui.style().visuals.panel_fill)
                        .max_size(ui.available_size());

                    ui.centered_and_justified(|ui| {
                        ui.add(image_widget);
                    });
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label(self.tr(
                            app_state.language,
                            "画像をドラッグ＆ドロップするか、ファイルメニューから開いてください。",
                            "Drag & drop images or open from File menu.",
                        ));
                    });
                }
            })
            .response;
        *app_state.is_pointer_over_central_panel = response.hovered();
    }

    /// アプリケーションの下部パネル（ページスライダー）を構築します。
    fn bottom_panel(
        &mut self,
        ctx: &Context,
        app_state: &mut ComicViewerAppState,
    ) -> Vec<UiCommand> {
        let mut commands = Vec::new();
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let (current_file_label, current_page, max_pages) =
                    if let Some(file) = &app_state.content_file {
                        let name = file.path.file_name().unwrap_or_default().to_string_lossy();
                        let (current, total) = match &file.file_type {
                            FileType::Zip(zip_file) if !zip_file.entries.is_empty() => {
                                (*app_state.current_page_index, zip_file.entries.len())
                            }
                            _ => (0, 1),
                        };
                        (name.to_string(), current, total)
                    } else {
                        (
                            self.tr(
                                app_state.language,
                                "ファイルが開かれていません",
                                "No file open",
                            )
                            .to_string(),
                            0,
                            1,
                        )
                    };

                ui.label(current_file_label);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{}/{}", current_page + 1, max_pages));

                    let mut page_slider_index = *app_state.current_page_index;
                    let slider =
                        egui::Slider::new(&mut page_slider_index, 0..=max_pages.saturating_sub(1))
                            .text(self.tr(app_state.language, "ページ", "Page"))
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
