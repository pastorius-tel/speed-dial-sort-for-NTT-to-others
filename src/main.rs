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
// CSV電話帳処理ツール - Rust実装
// Python版 (tkinter + pandas) をRust (egui + csv) に変換
// ======================================================

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod errors;
mod encoding;
mod text_cleaner;
mod csv_processor;
mod app;

use eframe::egui;
use log::info;

fn main() {
    // ログをファイルにも出力（csv_processor.log）
    // パニック時もファイルに記録されるためデバッグに有用
    let log_file = std::fs::OpenOptions::new()
        .create(true).append(true)
        .open("csv_processor.log")
        .ok();

    let mut builder = env_logger::Builder::new();
    builder.filter_level(log::LevelFilter::Info)
           .format_timestamp_secs();

    if let Some(file) = log_file {
        builder.target(env_logger::Target::Pipe(Box::new(file)));
    }
    builder.init();

    // パニック時にログへ書き込むフック
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("PANIC: {}", info);
        default_hook(info);
    }));

    info!("CSV電話帳処理ツールを開始します");

    // ウィンドウアイコン設定（タイトルバー・タスクバー用）
    let icon = load_window_icon();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("CSV電話帳処理ツール")
        .with_inner_size([900.0, 700.0])
        .with_min_inner_size([700.0, 500.0]);

    if let Some(icon_data) = icon {
        viewport = viewport.with_icon(icon_data);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let _ = eframe::run_native(
        "CSV電話帳処理ツール",
        native_options,
        Box::new(|cc| Box::new(app::CsvPhonebookApp::new(cc))),
    );

    info!("アプリケーションを終了します");
}

/// icon.ico をウィンドウアイコンとして読み込む
/// exe と同じフォルダに icon.ico を置いておくと実行時にも使用される
fn load_window_icon() -> Option<egui::IconData> {
    // exe と同じディレクトリの icon.ico を探す
    let exe_dir = std::env::current_exe().ok()?
        .parent()?
        .to_path_buf();
    let icon_path = exe_dir.join("icon.ico");

    let data = std::fs::read(&icon_path).ok()?;

    // ico ファイルをデコードして RGBA に変換
    let cursor = std::io::Cursor::new(data);
    let icon_dir = ico::IconDir::read(cursor).ok()?;
    let entry = icon_dir.entries().first()?;
    let image = entry.decode().ok()?;

    Some(egui::IconData {
        rgba: image.rgba_data().to_vec(),
        width:  image.width(),
        height: image.height(),
    })
}
