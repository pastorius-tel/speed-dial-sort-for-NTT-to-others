// SPDX-License-Identifier: MIT
// Copyright (c) 2024 TNC
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

// ======================================================
// app.rs - GUIアプリケーション本体
// ======================================================

use crate::csv_processor::{
    safe_read_csv, process_csv_data,
    CsvData, ProcessConfig, ProcessResult, ProgressInfo,
};
use crossbeam_channel::{bounded, Receiver};
use eframe::egui::{self, Color32, RichText, ScrollArea, Ui};
use log::{error, info};
use rfd::FileDialog;
use std::path::PathBuf;
use std::thread;

#[derive(Debug, Clone, PartialEq)]
enum AppScreen {
    FileSelect,
    ColumnSelect,
    Settings,
    Processing,
    Result,
    Error(String),
}

pub struct CsvPhonebookApp {
    screen: AppScreen,
    file_path: Option<PathBuf>,
    csv_data: Option<CsvData>,
    memory_col: String,
    name_col: String,
    phone_col: String,
    group_col: String,
    remove_spaces: bool,
    extract_nuisance: bool,
    add_sharp: bool,
    remove_terms: Vec<String>,
    remove_terms_enabled: Vec<bool>,
    settings_new_term: String,
    /// 名前文字幅上限: 16 or 20
    name_length_limit: usize,
    yomi_col: String,
    export_nakayo: bool,
    export_nakayo_phonebook: bool,
    progress: ProgressInfo,
    progress_rx: Option<Receiver<ProgressInfo>>,
    processing_thread: Option<thread::JoinHandle<Result<ProcessResult, String>>>,
    result: Option<ProcessResult>,
    result_messages_text: String,
}

impl CsvPhonebookApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // ── 日本語フォント設定 ──────────────────────────────
        // Windows / macOS / Linux のシステムフォントを優先的に読み込む
        // 外部フォントファイル不要でビルドできる構成
        setup_japanese_fonts(&cc.egui_ctx);

        let default_terms = vec![
            "有限会社".to_string(), "株式会社".to_string(),
            "㈱".to_string(), "(株)".to_string(), "(有)".to_string(),
            "合同会社".to_string(), "合資会社".to_string(),
        ];
        let len = default_terms.len();
        Self {
            screen: AppScreen::FileSelect,
            file_path: None,
            csv_data: None,
            memory_col: String::new(),
            name_col: String::new(),
            phone_col: String::new(),
            group_col: String::new(),
            remove_spaces: true,
            extract_nuisance: true,
            add_sharp: false,
            remove_terms: default_terms,
            remove_terms_enabled: vec![true; len],
            settings_new_term: String::new(),
            name_length_limit: 16,
            yomi_col: String::new(),
            export_nakayo: false,
            export_nakayo_phonebook: false,
            progress: ProgressInfo {
                value: 0.0,
                message: "処理を開始しています...".to_string(),
                detail: String::new(),
                cancelled: false,
            },
            progress_rx: None,
            processing_thread: None,
            result: None,
            result_messages_text: String::new(),
        }
    }

    fn selected_remove_terms(&self) -> Vec<String> {
        self.remove_terms.iter()
            .zip(self.remove_terms_enabled.iter())
            .filter(|(_, &en)| en)
            .map(|(t, _)| t.clone())
            .collect()
    }

    fn open_file(&mut self, path: PathBuf) {
        info!("ファイル処理開始: {}", path.display());
        match safe_read_csv(&path) {
            Ok(data) => {
                self.file_path = Some(path);
                self.csv_data = Some(data);
                self.screen = AppScreen::ColumnSelect;
            }
            Err(e) => {
                error!("読み込みエラー: {}", e);
                self.screen = AppScreen::Error(e.to_string());
            }
        }
    }

    fn draw_file_select(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.heading("CSV電話帳処理ツール");
            ui.add_space(30.0);
            ui.label("CSVファイルを選択して処理を開始してください");
            ui.add_space(20.0);
            if ui.button(RichText::new("📂 CSVファイルを選択").size(16.0)).clicked() {
                if let Some(path) = FileDialog::new()
                    .add_filter("CSV files", &["csv"])
                    .add_filter("All files", &["*"])
                    .pick_file()
                {
                    self.open_file(path);
                }
            }
        });
    }



    fn draw_column_select(&mut self, ui: &mut Ui) {
        let data = match &self.csv_data {
            Some(d) => d.clone(),
            None => return,
        };

        ui.heading("CSVデータの先頭5行 (プレビュー)");
        ui.add_space(8.0);

        // データプレビュー (egui 0.27: id_source)
        ScrollArea::horizontal()
            .id_source("preview_scroll")
            .max_height(160.0)
            .show(ui, |ui| {
                egui::Grid::new("preview_grid")
                    .striped(true)
                    .min_col_width(100.0)
                    .show(ui, |ui| {
                        for h in &data.headers {
                            ui.label(RichText::new(h).strong());
                        }
                        ui.end_row();
                        for row in data.rows.iter().take(5) {
                            for cell in row { ui.label(cell); }
                            ui.end_row();
                        }
                    });
            });

        ui.add_space(10.0);
        ui.separator();

        ui.group(|ui| {
            ui.label(RichText::new("処理設定").strong().size(14.0));
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("会社名処理:").strong());
                if ui.button("🔧 削除する会社名を設定").clicked() {
                    self.screen = AppScreen::Settings;
                }
            });
            ui.add_space(6.0);

            // egui 0.27: ComboBox::from_id_source (not from_id_salt)
            let col_options: Vec<String> = std::iter::once(String::new())
                .chain(data.headers.clone())
                .collect();

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("メモリ番号の列:  ").monospace());
                egui::ComboBox::from_id_source("memory_col")
                    .selected_text(&self.memory_col)
                    .show_ui(ui, |ui: &mut egui::Ui| {
                        for opt in &col_options {
                            ui.selectable_value(&mut self.memory_col, opt.clone(), opt);
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("名前・名称の列:  ").monospace());
                egui::ComboBox::from_id_source("name_col")
                    .selected_text(&self.name_col)
                    .show_ui(ui, |ui: &mut egui::Ui| {
                        for opt in &col_options {
                            ui.selectable_value(&mut self.name_col, opt.clone(), opt);
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("電話番号の列:    ").monospace());
                egui::ComboBox::from_id_source("phone_col")
                    .selected_text(&self.phone_col)
                    .show_ui(ui, |ui: &mut egui::Ui| {
                        for opt in &col_options {
                            ui.selectable_value(&mut self.phone_col, opt.clone(), opt);
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("グループ番号の列:").monospace());
                egui::ComboBox::from_id_source("group_col")
                    .selected_text(&self.group_col)
                    .show_ui(ui, |ui: &mut egui::Ui| {
                        for opt in &col_options {
                            ui.selectable_value(&mut self.group_col, opt.clone(), opt);
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("ヨミガナの列:    ").monospace());
                egui::ComboBox::from_id_source("yomi_col")
                    .selected_text(&self.yomi_col)
                    .show_ui(ui, |ui: &mut egui::Ui| {
                        for opt in &col_options {
                            ui.selectable_value(&mut self.yomi_col, opt.clone(), opt);
                        }
                    });
            });

            ui.add_space(6.0);
            ui.checkbox(&mut self.remove_spaces,    "名前から半角・全角スペースを削除");
            ui.checkbox(&mut self.extract_nuisance, "グループ番号9を迷惑電話ファイルとして保存");
            if self.extract_nuisance {
                ui.indent("nakayo_indent", |ui| {
                    ui.checkbox(&mut self.export_nakayo,
                        "ナカヨ迷惑電話形式でも出力する（項番,電話番号 / Shift-JIS）");
                });
            }
            ui.checkbox(&mut self.export_nakayo_phonebook,
                "処理済みデータをナカヨ電話帳.csv形式でも出力する");
            ui.checkbox(&mut self.add_sharp,        "電話番号の最後に # を付加する");

            ui.add_space(6.0);
            ui.label(RichText::new("名前の文字数上限:").strong());
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.name_length_limit, 16,
                    "16カウント（全角8文字 / 半角16文字）");
                ui.add_space(20.0);
                ui.radio_value(&mut self.name_length_limit, 20,
                    "20カウント（全角10文字 / 半角20文字）");
            });
        });

        ui.add_space(10.0);

        ui.horizontal(|ui| {
            let can_process = !self.memory_col.is_empty()
                || !self.name_col.is_empty()
                || !self.phone_col.is_empty()
                || !self.group_col.is_empty();

            if ui.add_enabled(
                can_process,
                egui::Button::new(RichText::new("▶ 処理実行").size(14.0))
                    .fill(Color32::from_rgb(144, 238, 144)),
            ).clicked() {
                self.start_processing();
            }

            if !can_process {
                ui.label(
                    RichText::new("※ 少なくとも1つの列を選択してください")
                        .color(Color32::RED).small()
                );
            }

            if ui.add(
                egui::Button::new(RichText::new("✖ キャンセル").size(14.0))
                    .fill(Color32::from_rgb(255, 160, 160)),
            ).clicked() {
                self.screen = AppScreen::FileSelect;
                self.csv_data = None;
                self.file_path = None;
            }
        });
    }

    fn draw_settings(&mut self, ui: &mut Ui) {
        ui.heading("削除する会社名の編集");
        ui.label(RichText::new("チェックを外すと削除対象から除外されます").small().color(Color32::GRAY));
        ui.add_space(8.0);

        ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            let mut to_delete: Option<usize> = None;
            for i in 0..self.remove_terms.len() {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.remove_terms_enabled[i], &self.remove_terms[i]);
                    if ui.small_button("🗑 削除").clicked() {
                        to_delete = Some(i);
                    }
                });
            }
            if let Some(idx) = to_delete {
                self.remove_terms.remove(idx);
                self.remove_terms_enabled.remove(idx);
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new("新しい削除対象を追加:").strong());
        ui.horizontal(|ui| {
            let response = ui.text_edit_singleline(&mut self.settings_new_term);
            let add_clicked = ui.button("追加").clicked();
            let enter_pressed = response.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter));

            if add_clicked || enter_pressed {
                let term = self.settings_new_term.trim().to_string();
                if !term.is_empty() && !self.remove_terms.contains(&term) {
                    info!("削除対象を追加: {}", term);
                    self.remove_terms.push(term);
                    self.remove_terms_enabled.push(true);
                    self.settings_new_term.clear();
                }
            }
        });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("🔄 デフォルトに戻す").clicked() {
                self.remove_terms = vec![
                    "有限会社".to_string(), "株式会社".to_string(),
                    "㈱".to_string(), "(株)".to_string(), "(有)".to_string(),
                    "合同会社".to_string(), "合資会社".to_string(),
                ];
                self.remove_terms_enabled = vec![true; self.remove_terms.len()];
            }
            if ui.button("✔ OK").clicked() {
                self.screen = AppScreen::ColumnSelect;
            }
            if ui.button("キャンセル").clicked() {
                self.screen = AppScreen::ColumnSelect;
            }
        });
    }

    fn draw_processing(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if let Some(ref rx) = self.progress_rx {
            while let Ok(info) = rx.try_recv() {
                self.progress = info;
            }
        }

        let is_done = self.processing_thread
            .as_ref()
            .map(|t| t.is_finished())
            .unwrap_or(false);

        if is_done {
            if let Some(t) = self.processing_thread.take() {
                match t.join() {
                    Ok(Ok(result)) => {
                        self.result_messages_text = result.messages.join("\n");
                        self.result = Some(result);
                        self.screen = AppScreen::Result;
                    }
                    Ok(Err(e)) => self.screen = AppScreen::Error(e),
                    Err(_) => self.screen = AppScreen::Error("処理スレッドがパニックしました".to_string()),
                }
            }
            return;
        }

        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.heading("処理中...");
            ui.add_space(20.0);
            ui.label(&self.progress.message);
            ui.add(
                egui::ProgressBar::new(self.progress.value / 100.0)
                    .desired_width(350.0)
                    .show_percentage(),
            );
            ui.add_space(8.0);
            if !self.progress.detail.is_empty() {
                ui.label(RichText::new(&self.progress.detail).small().color(Color32::GRAY));
            }
            ui.add_space(20.0);
            if ui.button("キャンセル").clicked() {
                self.progress.cancelled = true;
                self.screen = AppScreen::ColumnSelect;
            }
        });

        ctx.request_repaint();
    }

    fn draw_result(&mut self, ui: &mut Ui) {
        ui.heading("✅ 処理完了");
        ui.add_space(10.0);

        let mut display_text = self.result_messages_text.clone();
        ScrollArea::vertical().max_height(350.0).show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut display_text)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .interactive(false),
            );
        });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("📁 保存フォルダを開く").clicked() {
                if let Some(ref result) = self.result {
                    let folder = result.saved_path.parent()
                        .unwrap_or(result.saved_path.as_path());
                    open_folder(folder);
                }
            }
            if ui.button("🔄 別のファイルを処理する").clicked() {
                self.reset_to_file_select();
            }
            if ui.button("❌ 終了").clicked() {
                std::process::exit(0);
            }
        });
    }

    fn draw_error(&mut self, ui: &mut Ui, error_msg: String) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.heading(RichText::new("❌ エラーが発生しました").color(Color32::RED));
            ui.add_space(20.0);
            ui.label(&error_msg);
            ui.add_space(20.0);
            if ui.button("戻る").clicked() {
                self.reset_to_file_select();
            }
        });
    }

    fn start_processing(&mut self) {
        let data = match self.csv_data.clone() {
            Some(d) => d,
            None => return,
        };
        let file_path = match self.file_path.clone() {
            Some(p) => p,
            None => return,
        };

        let config = ProcessConfig {
            memory_col: if self.memory_col.is_empty() { None } else { Some(self.memory_col.clone()) },
            name_col:   if self.name_col.is_empty()   { None } else { Some(self.name_col.clone()) },
            phone_col:  if self.phone_col.is_empty()  { None } else { Some(self.phone_col.clone()) },
            group_col:  if self.group_col.is_empty()  { None } else { Some(self.group_col.clone()) },
            remove_spaces: self.remove_spaces,
            extract_nuisance: self.extract_nuisance,
            add_sharp: self.add_sharp,
            remove_terms: self.selected_remove_terms(),
            name_length_limit: self.name_length_limit,
            export_nakayo: self.export_nakayo,
            export_nakayo_phonebook: self.export_nakayo_phonebook,
            yomi_col: if self.yomi_col.is_empty() { None } else { Some(self.yomi_col.clone()) },
        };

        info!(
            "設定: メモリ={:?}, 名前={:?}, 電話={:?}, グループ={:?}, #付加={}",
            config.memory_col, config.name_col, config.phone_col,
            config.group_col, config.add_sharp
        );

        let (tx, rx) = bounded::<ProgressInfo>(100);
        self.progress_rx = Some(rx);
        self.progress = ProgressInfo {
            value: 0.0,
            message: "処理を開始しています...".to_string(),
            detail: String::new(),
            cancelled: false,
        };

        let handle = thread::spawn(move || {
            process_csv_data(data, &file_path, &config, &tx)
                .map_err(|e| e.to_string())
        });

        self.processing_thread = Some(handle);
        self.screen = AppScreen::Processing;
    }

    fn reset_to_file_select(&mut self) {
        self.screen = AppScreen::FileSelect;
        self.csv_data = None;
        self.file_path = None;
        self.result = None;
        self.result_messages_text = String::new();
        self.memory_col = String::new();
        self.name_col = String::new();
        self.phone_col = String::new();
        self.group_col = String::new();
        self.yomi_col = String::new();
    }
}

impl eframe::App for CsvPhonebookApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                let screen = self.screen.clone();
                match screen {
                    AppScreen::FileSelect    => self.draw_file_select(ui),
                    AppScreen::ColumnSelect  => self.draw_column_select(ui),
                    AppScreen::Settings      => self.draw_settings(ui),
                    AppScreen::Processing    => self.draw_processing(ui, ctx),
                    AppScreen::Result        => self.draw_result(ui),
                    AppScreen::Error(msg)    => self.draw_error(ui, msg),
                }
            });
        });
    }
}

fn open_folder(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    { let _ = std::process::Command::new("explorer").arg(path).spawn(); }
    #[cfg(target_os = "macos")]
    { let _ = std::process::Command::new("open").arg(path).spawn(); }
    #[cfg(target_os = "linux")]
    { let _ = std::process::Command::new("xdg-open").arg(path).spawn(); }
}

// ======================================================
// 日本語フォント設定
// Windows / macOS / Linux のシステムフォントを自動検索して読み込む
// ======================================================
fn setup_japanese_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // OS別のフォント候補（優先順）
    #[cfg(target_os = "windows")]
    let candidates = vec![
        r"C:\Windows\Fonts\msgothic.ttc",   // MS ゴシック
        r"C:\Windows\Fonts\meiryo.ttc",     // メイリオ
        r"C:\Windows\Fonts\YuGothM.ttc",    // 游ゴシック Medium
        r"C:\Windows\Fonts\YuGothR.ttc",    // 游ゴシック Regular
        r"C:\Windows\Fonts\msmincho.ttc",   // MS 明朝
    ];

    #[cfg(target_os = "macos")]
    let candidates = vec![
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/Library/Fonts/Osaka.ttf",
    ];

    #[cfg(target_os = "linux")]
    let candidates = vec![
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJKjp-Regular.otf",
        "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf",
        "/usr/share/fonts/truetype/takao-gothic/TakaoGothic.ttf",
    ];

    // 候補を順番に試して最初に読み込めたものを使用
    let mut loaded = false;
    for path in &candidates {
        if let Ok(font_data) = std::fs::read(path) {
            log::info!("日本語フォントを読み込みました: {}", path);
            fonts.font_data.insert(
                "japanese".to_owned(),
                egui::FontData::from_owned(font_data),
            );
            // Proportional（通常テキスト）と Monospace（等幅）の両方に追加
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("japanese".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("japanese".to_owned());
            loaded = true;
            break;
        }
    }

    if !loaded {
        log::warn!("日本語フォントが見つかりませんでした。環境変数 EGUI_FONT_PATH でフォントパスを指定できます。");
        // 環境変数でフォントパスを上書き指定できるフォールバック
        if let Ok(env_path) = std::env::var("EGUI_FONT_PATH") {
            if let Ok(font_data) = std::fs::read(&env_path) {
                log::info!("環境変数フォントを読み込みました: {}", env_path);
                fonts.font_data.insert(
                    "japanese".to_owned(),
                    egui::FontData::from_owned(font_data),
                );
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push("japanese".to_owned());
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .push("japanese".to_owned());
            }
        }
    }

    ctx.set_fonts(fonts);

    // ── フォントサイズ設定 ──────────────────────────────
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (egui::TextStyle::Heading,   egui::FontId::new(22.0, egui::FontFamily::Proportional)),
        (egui::TextStyle::Body,      egui::FontId::new(16.0, egui::FontFamily::Proportional)),
        (egui::TextStyle::Button,    egui::FontId::new(16.0, egui::FontFamily::Proportional)),
        (egui::TextStyle::Small,     egui::FontId::new(13.0, egui::FontFamily::Proportional)),
        (egui::TextStyle::Monospace, egui::FontId::new(15.0, egui::FontFamily::Monospace)),
    ].into();
    // ボタンや入力欄の余白も少し広げて見やすく
    style.spacing.item_spacing   = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    ctx.set_style(style);
}
