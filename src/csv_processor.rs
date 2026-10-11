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
// csv_processor.rs - CSV処理メインロジック
// Python版の process_csv_data() / safe_read_csv() に対応
// ======================================================

use crate::errors::{AppError, CsvProcessingError};
use crate::encoding::detect_encoding;
use crate::text_cleaner::{clean_company_names, process_phone_sharp, check_name_length};
use chrono::Local;
use encoding_rs;
use log::{info, error};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// CSV データの表現（列名 + 行データ）
#[derive(Clone, Debug)]
pub struct CsvData {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl CsvData {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self { headers, rows }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn col_count(&self) -> usize {
        self.headers.len()
    }

    /// 列インデックスを列名から取得
    pub fn col_index(&self, name: &str) -> Option<usize> {
        self.headers.iter().position(|h| h == name)
    }

}

/// 処理設定（Python版の apply_selection() の設定値に対応）
#[derive(Clone, Debug)]
pub struct ProcessConfig {
    pub memory_col: Option<String>,
    pub name_col: Option<String>,
    pub phone_col: Option<String>,
    pub group_col: Option<String>,
    pub remove_spaces: bool,
    pub extract_nuisance: bool,
    pub add_sharp: bool,
    pub remove_terms: Vec<String>,
    /// ヨミガナ列
    pub yomi_col: Option<String>,
    /// 名前・ヨミガナ共通の文字幅上限（16 or 20）
    pub name_length_limit: usize,
    /// グループ9をナカヨ迷惑電話形式で出力する
    pub export_nakayo: bool,
    /// 処理済みデータをナカヨ電話帳.csv形式でも出力する
    pub export_nakayo_phonebook: bool,
}

/// 進捗通知（Python版の ProgressWindow.update() に対応）
#[derive(Debug, Clone)]
pub struct ProgressInfo {
    pub value: f32,      // 0.0 - 100.0
    pub message: String,
    pub detail: String,
    pub cancelled: bool,
}

/// 処理結果メッセージ
#[derive(Debug, Clone)]
pub struct ProcessResult {
    pub messages: Vec<String>,
    pub saved_path: PathBuf,
}

/// ファイルサイズを取得（MB単位）
fn file_size_mb(path: &Path) -> f64 {
    fs::metadata(path)
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0)
}

/// 安全なCSV読み込み（Python版の safe_read_csv() に対応）
///
/// - 50MB未満: 一括読み込み
/// - 50MB以上: チャンク読み込みと同等の処理（Rustではストリーミング）
pub fn safe_read_csv(file_path: &Path) -> Result<CsvData, AppError> {
    let size_mb = file_size_mb(file_path);
    info!("ファイルサイズ: {:.2} MB", size_mb);

    // エンコーディング検出
    let encoding = detect_encoding(file_path)?;
    info!("使用エンコーディング: {}", encoding.name());

    // ファイル読み込み
    let raw_bytes = fs::read(file_path).map_err(|e| {
        AppError::Csv(CsvProcessingError::ReadError(e.to_string()))
    })?;

    if raw_bytes.is_empty() {
        return Err(AppError::Csv(CsvProcessingError::EmptyFile));
    }

    // エンコーディング変換
    let (decoded, _, _) = encoding.decode(&raw_bytes);
    let content = decoded.into_owned();

    // BOM除去（UTF-8 BOM対応）
    let content = content.trim_start_matches('\u{FEFF}');

    // CSV パース
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true) // 列数が行によって異なる場合も許容
        .from_reader(content.as_bytes());

    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| AppError::Csv(CsvProcessingError::InvalidFormat(e.to_string())))?
        .iter()
        .map(|h| h.trim().to_string())
        .collect();

    if headers.is_empty() {
        return Err(AppError::Csv(CsvProcessingError::EmptyFile));
    }

    let col_count = headers.len();
    let mut rows: Vec<Vec<String>> = Vec::new();

    for result in reader.records() {
        let record = result.map_err(|e| {
            AppError::Csv(CsvProcessingError::InvalidFormat(e.to_string()))
        })?;

        let mut row: Vec<String> = record
            .iter()
            .map(|f| f.trim().to_string())
            .collect();

        // 列数を揃える（短い行はNaN相当の空文字列で埋める）
        row.resize(col_count, String::new());
        rows.push(row);
    }

    info!("データを読み込みました: {}行, {}列", rows.len(), col_count);
    Ok(CsvData::new(headers, rows))
}

/// CSV処理のメイン関数（Python版の process_csv_data() に対応）
///
/// # 引数
/// - `data`: CSVデータ
/// - `file_path`: 元ファイルパス
/// - `config`: 処理設定
/// - `progress_tx`: 進捗通知チャンネル
///
/// Python版と同等の処理順:
/// 1. メモリ番号処理
/// 2. 名前クリーニング
/// 3. 電話番号処理（#付加/削除）
/// 4. グループ処理・迷惑電話抽出
/// 5. 重複電話番号検出
/// 6. ファイル保存
pub fn process_csv_data(
    data: CsvData,
    file_path: &Path,
    config: &ProcessConfig,
    progress_tx: &crossbeam_channel::Sender<ProgressInfo>,
) -> Result<ProcessResult, AppError> {

    let mut df = data.clone();
    let dir = file_path.parent().unwrap_or(Path::new("."));
    let base_name = file_path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let mut messages: Vec<String> = Vec::new();

    let send_progress = |value: f32, msg: &str, detail: &str| {
        let _ = progress_tx.send(ProgressInfo {
            value,
            message: msg.to_string(),
            detail: detail.to_string(),
            cancelled: false,
        });
    };

    // ────────────────────────────────────────
    // ステップ1: メモリ番号の処理
    // Python: df_copy[memory_col] = df_copy[memory_col].str.zfill(4)
    //         + 0000〜9999 の全番号リストとマージ
    // ────────────────────────────────────────
    send_progress(10.0, "メモリ番号を処理中...", &format!("処理対象: {}行", df.row_count()));

    if let Some(ref mem_col) = config.memory_col.clone() {
        if let Some(col_idx) = df.col_index(mem_col) {
            // 既存データを 0埋め4桁 に正規化
            for row in df.rows.iter_mut() {
                let val = row[col_idx].trim().to_string();
                if !val.is_empty() {
                    // 数値部分だけを抽出して4桁0埋め
                    if let Ok(n) = val.parse::<u32>() {
                        row[col_idx] = format!("{:04}", n);
                    }
                }
            }

            // 0000〜9999 の全番号リストとマージ
            // Python: pd.merge(all_numbers, df_copy, on=memory_col, how="left")
            // 修正#1: 同一メモリ番号が複数ある場合は最初の行を優先
            // collect()はHashMapで後勝ちになるため、折りたたみで先勝ちに変更
            let mut existing: HashMap<String, Vec<String>> = HashMap::new();
            for r in df.rows.iter().filter(|r| !r[col_idx].is_empty()) {
                existing.entry(r[col_idx].clone()).or_insert_with(|| r.clone());
            }

            let empty_row: Vec<String> = vec![String::new(); df.col_count()];

            let new_rows: Vec<Vec<String>> = (0u32..10000).map(|i| {
                let key = format!("{:04}", i);
                if let Some(row) = existing.get(&key) {
                    row.clone()
                } else {
                    let mut r = empty_row.clone();
                    r[col_idx] = key;
                    r
                }
            }).collect();

            df.rows = new_rows;
            info!("メモリ番号処理完了: {}行", df.row_count());
        }
    }

    // ────────────────────────────────────────
    // ステップ2: 名前・名称のクリーニング
    // Python: df_copy[name_col].apply(clean_company_names)
    // ────────────────────────────────────────
    send_progress(40.0, "名前・名称を処理中...", "");

    if let Some(ref name_col) = config.name_col.clone() {
        if let Some(col_idx) = df.col_index(name_col) {
            let total = df.rows.len();
            for (i, row) in df.rows.iter_mut().enumerate() {
                let cleaned = clean_company_names(
                    &row[col_idx],
                    &config.remove_terms,
                    config.remove_spaces,
                );
                row[col_idx] = cleaned;

                if i % 1000 == 0 && i > 0 {
                    let pct = 40.0 + (i as f32 / total as f32) * 20.0;
                    send_progress(pct, "名前処理中...", &format!("{}/{}", i, total));
                }
            }
            info!("名前処理完了: {}件", total);
        }
    }

    // ────────────────────────────────────────
    // ステップ3: 電話番号の # 付加/削除
    // Python: df_copy[phone_col].apply(process_phone_sharp)
    // ────────────────────────────────────────
    send_progress(60.0, "電話番号を処理中...", "");

    if let Some(ref phone_col) = config.phone_col.clone() {
        if let Some(col_idx) = df.col_index(phone_col) {
            for row in df.rows.iter_mut() {
                row[col_idx] = process_phone_sharp(&row[col_idx], config.add_sharp);
            }
            info!("電話番号処理完了: #付加={}", config.add_sharp);
        }
    }

    // ────────────────────────────────────────
    // ステップ4: グループ処理・迷惑電話抽出
    // Python: df_copy[group_col == "9"] → 迷惑電話ファイル保存
    // ────────────────────────────────────────
    send_progress(70.0, "グループ番号を処理中...", "");

    if let Some(ref group_col) = config.group_col.clone() {
        if let Some(group_idx) = df.col_index(group_col) {
            if config.extract_nuisance {
                // グループ9のデータを抽出
                let nuisance_rows: Vec<Vec<String>> = df.rows.iter()
                    .filter(|r| r.get(group_idx).map(|v| v == "9").unwrap_or(false))
                    .cloned()
                    .collect();

                if !nuisance_rows.is_empty() {
                    // 迷惑電話ファイル保存
                    let dt = Local::now().format("%Y%m%d_%H%M%S");
                    let nuisance_path = dir.join(format!("{}_迷惑電話_{}.csv", base_name, dt));

                    match save_csv_data(&df.headers, &nuisance_rows, &nuisance_path) {
                        Ok(_) => {
                            let msg = format!("迷惑電話ファイルを保存: {}", nuisance_path.display());
                            info!("{}", msg);
                            messages.push(msg);
                        }
                        Err(e) => {
                            let msg = format!("迷惑電話ファイル保存エラー: {}", e);
                            error!("{}", msg);
                            messages.push(msg);
                        }
                    }

                    // グループ9のデータをクリア（メモリ番号とグループ番号列以外）
                    // Python: df_copy.loc[group == "9", cols_to_clear] = ""
                    let mem_idx = config.memory_col.as_ref().and_then(|c| df.col_index(c));
                    for row in df.rows.iter_mut() {
                        if row.get(group_idx).map(|v| v == "9").unwrap_or(false) {
                            for (i, cell) in row.iter_mut().enumerate() {
                                if i != group_idx && Some(i) != mem_idx {
                                    *cell = String::new();
                                }
                            }
                        }
                    }

                    info!("迷惑電話抽出完了: {}件", nuisance_rows.len());

                    // ── ナカヨ迷惑電話形式でも出力（処理済みデータから）
                    // メモリ番号はすでに0埋め・マージ済みのデータを使用
                    if config.export_nakayo {
                        let phone_idx = config.phone_col.as_ref().and_then(|c| df.col_index(c));
                        match write_nakayo_format(&nuisance_rows, mem_idx, phone_idx, dir, base_name) {
                            Ok((path, count)) => {
                                let msg = format!("ナカヨ迷惑電話形式を保存: {}件 → {}", count, path.display());
                                info!("{}", msg);
                                messages.push(msg);
                            }
                            Err(e) => {
                                let msg = format!("ナカヨ形式保存エラー: {}", e);
                                error!("{}", msg);
                                messages.push(msg);
                            }
                        }
                    }
                }
            }
        }
    }

    // ────────────────────────────────────────
    // ステップ5: 重複電話番号の検出
    // Python: duplicates検出 → 重複電話番号レポートファイル保存
    // ────────────────────────────────────────
    send_progress(80.0, "重複電話番号を検出中...", "");

    if let (Some(ref phone_col), Some(ref mem_col)) = (&config.phone_col, &config.memory_col) {
        if let (Some(phone_idx), Some(mem_idx)) = (df.col_index(phone_col), df.col_index(mem_col)) {
            // 電話番号→メモリ番号リスト のマップを構築
            let mut phone_map: HashMap<String, Vec<String>> = HashMap::new();

            for row in &df.rows {
                let phone = row.get(phone_idx).map(|s| s.trim()).unwrap_or("").to_string();
                let mem = row.get(mem_idx).map(|s| s.trim()).unwrap_or("").to_string();

                if !phone.is_empty() && !mem.is_empty() {
                    phone_map.entry(phone).or_default().push(mem);
                }
            }

            // 重複（2件以上）のみ抽出
            let duplicates: HashMap<&String, &Vec<String>> = phone_map.iter()
                .filter(|(_, mems)| mems.len() > 1)
                .collect();

            if !duplicates.is_empty() {
                let dt = Local::now().format("%Y%m%d_%H%M%S");
                let dup_path = dir.join(format!("{}_重複電話番号_{}.txt", base_name, dt));

                match write_duplicate_report(&duplicates, &dup_path) {
                    Ok(_) => {
                        let msg = format!("重複電話番号ファイルを保存: {}", dup_path.display());
                        info!("{}", msg);
                        messages.push(msg);
                        info!("重複電話番号検出完了: {}件", duplicates.len());
                    }
                    Err(e) => {
                        let msg = format!("重複電話番号ファイル保存エラー: {}", e);
                        error!("{}", msg);
                        messages.push(msg);
                    }
                }
            } else {
                messages.push("重複する電話番号は見つかりませんでした".to_string());
            }
        }
    }

    // ────────────────────────────────────────
    // ステップ5.5: 名前の文字数超過チェック
    // 全角=2、半角=1 で計算して16超過のセルをレポート
    // ────────────────────────────────────────
    send_progress(85.0, "名前の文字数をチェック中...", "");

    if let Some(ref name_col) = config.name_col {
        if let Some(name_idx) = df.col_index(name_col) {
            // 超過セル収集: (メモリ番号 or 行番号, 名前, 表示幅) のリスト
            let mem_idx = config.memory_col.as_ref().and_then(|c| df.col_index(c));

            let over_rows: Vec<(String, String, usize, usize)> = df.rows.iter()
                .enumerate()
                .filter_map(|(i, row)| {
                    let name = row.get(name_idx).map(|s| s.as_str()).unwrap_or("");
                    if name.is_empty() { return None; }
                    let info = check_name_length(name, config.name_length_limit);
                    if info.over {
                        let key = mem_idx
                            .and_then(|mi| row.get(mi))
                            .filter(|s| !s.is_empty())
                            .cloned()
                            .unwrap_or_else(|| format!("行{}", i + 2)); // +2: ヘッダー分+1
                        Some((key, name.to_string(), info.width, info.over_by))
                    } else {
                        None
                    }
                })
                .collect();

            if !over_rows.is_empty() {
                let dt = Local::now().format("%Y%m%d_%H%M%S");
                let over_path = dir.join(format!("{}_名前文字数超過_{}.txt", base_name, dt));

                match write_name_length_report(&over_rows, &over_path, config.name_length_limit) {
                    Ok(_) => {
                        let msg = format!(
                            "名前文字数超過: {}件 → {}",
                            over_rows.len(), over_path.display()
                        );
                        info!("{}", msg);
                        messages.push(msg);
                    }
                    Err(e) => {
                        let msg = format!("文字数超過レポート保存エラー: {}", e);
                        error!("{}", msg);
                        messages.push(msg);
                    }
                }
            } else {
                messages.push("文字数超過の名前はありませんでした".to_string());
            }
        }
    }

    // ────────────────────────────────────────
    // ステップ5.6: ヨミガナの文字数超過チェック
    // 名前と同じルール（全角=2・半角=1、上限は共通設定）
    // ────────────────────────────────────────
    send_progress(87.0, "ヨミガナの文字数をチェック中...", ""); // 修正#3: 進捗通知追加
    if let Some(ref yomi_col) = config.yomi_col {
        if let Some(yomi_idx) = df.col_index(yomi_col) {
            let mem_idx = config.memory_col.as_ref().and_then(|c| df.col_index(c));

            let over_rows: Vec<(String, String, usize, usize)> = df.rows.iter()
                .enumerate()
                .filter_map(|(i, row)| {
                    let yomi = row.get(yomi_idx).map(|s| s.as_str()).unwrap_or("");
                    if yomi.is_empty() { return None; }
                    let info = check_name_length(yomi, config.name_length_limit);
                    if info.over {
                        let key = mem_idx
                            .and_then(|mi| row.get(mi))
                            .filter(|s| !s.is_empty())
                            .cloned()
                            .unwrap_or_else(|| format!("行{}", i + 2));
                        Some((key, yomi.to_string(), info.width, info.over_by))
                    } else {
                        None
                    }
                })
                .collect();

            if !over_rows.is_empty() {
                let dt = Local::now().format("%Y%m%d_%H%M%S");
                let over_path = dir.join(format!("{}_ヨミガナ文字数超過_{}.txt", base_name, dt));

                match write_name_length_report(&over_rows, &over_path, config.name_length_limit) {
                    Ok(_) => {
                        let msg = format!(
                            "ヨミガナ文字数超過: {}件 → {}",
                            over_rows.len(), over_path.display()
                        );
                        info!("{}", msg);
                        messages.push(msg);
                    }
                    Err(e) => {
                        let msg = format!("ヨミガナ文字数超過レポート保存エラー: {}", e);
                        error!("{}", msg);
                        messages.push(msg);
                    }
                }
            } else {
                messages.push("文字数超過のヨミガナはありませんでした".to_string());
            }
        }
    }

    // ────────────────────────────────────────
    // ステップ6: 処理済みCSV保存
    // Python: df.to_csv(save_path, index=False, encoding="utf-8-sig")
    // ────────────────────────────────────────
    send_progress(90.0, "ファイルを保存中...", "");

    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let save_path = dir.join(format!("{}_processed_{}.csv", base_name, timestamp));

    save_csv_data(&df.headers, &df.rows, &save_path)
        .map_err(|e| AppError::Csv(CsvProcessingError::SaveFailed(e.to_string())))?;

    send_progress(98.0, "保存完了", "");

    // 統計情報生成（Python: generate_statistics() に対応）
    let stats = generate_statistics(&df, file_path);
    messages.insert(0, format!("CSVファイルを保存しました: {}", save_path.display()));
    messages.extend(stats);

    // ────────────────────────────────────────
    // ステップ7: ナカヨ電話帳.csv形式での出力
    // 処理済みデータ（名前クリーニング・メモリ番号整列済み）を
    // ナカヨ電話機の電話帳インポート形式に変換して出力
    // ────────────────────────────────────────
    if config.export_nakayo_phonebook {
        send_progress(93.0, "ナカヨ電話帳形式を生成中...", "");

        let mem_idx   = config.memory_col.as_ref().and_then(|c| df.col_index(c));
        let name_idx  = config.name_col.as_ref().and_then(|c| df.col_index(c));
        let yomi_idx  = config.yomi_col.as_ref().and_then(|c| df.col_index(c));
        let phone_idx = config.phone_col.as_ref().and_then(|c| df.col_index(c));
        let group_idx = config.group_col.as_ref().and_then(|c| df.col_index(c));

        let ts7 = Local::now().format("%Y%m%d_%H%M%S");
        let pb_path = dir.join(format!("{}_ナカヨ電話帳_{}.csv", base_name, ts7));

        match write_nakayo_phonebook(
            &df, &pb_path,
            mem_idx, name_idx, yomi_idx, phone_idx, group_idx,
        ) {
            Ok(count) => {
                let msg = format!("ナカヨ電話帳.csv形式を保存: {}件 → {}", count, pb_path.display());
                info!("{}", msg);
                messages.push(msg);
            }
            Err(e) => {
                let msg = format!("ナカヨ電話帳.csv保存エラー: {}", e);
                error!("{}", msg);
                messages.push(msg);
            }
        }
    }

    send_progress(100.0, "処理完了", "");

    info!("ファイル保存完了: {}", save_path.display());

    Ok(ProcessResult {
        messages,
        saved_path: save_path,
    })
}

/// CSV データをファイルに保存（UTF-8 BOM付き）
/// Python: df.to_csv(path, encoding="utf-8-sig") に対応
fn save_csv_data(
    headers: &[String],
    rows: &[Vec<String>],
    path: &Path,
) -> Result<(), std::io::Error> {
    let file = fs::File::create(path)?;

    // UTF-8 BOM 書き込み（Excel対応）
    let mut writer = std::io::BufWriter::new(file);
    writer.write_all(&[0xEF, 0xBB, 0xBF])?;

    let mut csv_writer = csv::WriterBuilder::new()
        .from_writer(writer);

    // ヘッダー書き込み
    csv_writer.write_record(headers)?;

    // データ行書き込み
    for row in rows {
        csv_writer.write_record(row)?;
    }

    csv_writer.flush()?;
    Ok(())
}

/// 重複電話番号レポートの書き込み
/// Python版の dup_file 書き込み処理に対応
fn write_duplicate_report(
    duplicates: &HashMap<&String, &Vec<String>>,
    path: &Path,
) -> Result<(), std::io::Error> {
    let mut file = fs::File::create(path)?;

    writeln!(file, "重複電話番号レポート")?;
    writeln!(file, "作成日時: {}", Local::now().format("%Y/%m/%d %H:%M:%S"))?;
    writeln!(file, "{}", "=".repeat(50))?;
    writeln!(file)?;

    let mut sorted_phones: Vec<&&String> = duplicates.keys().collect();
    sorted_phones.sort();

    for phone in sorted_phones {
        let mems = duplicates[phone];
        let mut sorted_mems = mems.clone();
        sorted_mems.sort();

        writeln!(file, "電話番号: {}", phone)?;
        writeln!(file, "メモリ番号: {}", sorted_mems.join(", "))?;
        writeln!(file, "{}", "-".repeat(30))?;
    }

    Ok(())
}

/// 名前文字数超過レポートの書き込み
/// (メモリ番号or行番号, 名前, 表示幅) のリストをテキストファイルに出力
fn write_name_length_report(
    over_rows: &[(String, String, usize, usize)],
    path: &Path,
    limit: usize,
) -> Result<(), std::io::Error> {
    let mut file = fs::File::create(path)?;

    writeln!(file, "名前文字数超過レポート")?;
    writeln!(file, "作成日時: {}", Local::now().format("%Y/%m/%d %H:%M:%S"))?;
    writeln!(file, "制限: 全角{}文字 / 半角{}文字 / 混在は全角2・半角1で合計{}まで",
        limit / 2, limit, limit)?;
    writeln!(file, "{}", "=".repeat(50))?;
    writeln!(file)?;

    for (key, name, width, over_by) in over_rows {
        writeln!(file, "メモリ/行: {}", key)?;
        writeln!(file, "名前    : {}", name)?;
        writeln!(file, "文字幅  : {} ({}オーバー)", width, over_by)?;
        writeln!(file, "{}", "-".repeat(30))?;
    }

    Ok(())
}

/// ナカヨ迷惑電話形式でファイルに書き出す
///
/// 処理済みCSVデータ（メモリ番号0埋み・マージ済み）から変換するため
/// メモリ番号は4桁の文字列をそのまま数値化して使用する
///
/// 出力フォーマット（Shift-JIS / CRLF）:
///   項番,電話番号
///   0000,
///   0001,09012345678   ← グループ9かつ電話番号あり
///   ...
///   9999,
fn write_nakayo_format(
    nuisance_rows: &[Vec<String>],
    _mem_idx: Option<usize>,   // 頭詰めのためメモリ番号は使用しない
    phone_idx: Option<usize>,
    dir: &Path,
    base_name: &str,
) -> Result<(PathBuf, usize), std::io::Error> {
    use std::io::Write;

    let dt = Local::now().format("%Y%m%d_%H%M%S");
    let path = dir.join(format!("{}_迷惑電話_ナカヨ形式_{}.csv", base_name, dt));

    // 電話番号が空でない行のみ抽出し、頭詰めリストを作成
    // メモリ番号順ではなく、グループ9の電話番号を0000から連番で詰める
    let phones: Vec<String> = nuisance_rows.iter()
        .filter_map(|row| {
            let phone = phone_idx
                .and_then(|i| row.get(i))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            if phone.is_empty() { None } else { Some(phone) }
        })
        .collect();

    let count = phones.len();
    info!("ナカヨ迷惑電話形式（頭詰め）: {}件", count);

    // Shift-JIS / CRLF で書き出し
    let file = fs::File::create(&path)?;
    let mut writer = std::io::BufWriter::new(file);

    let (enc, _, _) = encoding_rs::SHIFT_JIS.encode("項番,電話番号\r\n");
    writer.write_all(&enc)?;

    for i in 0u32..10000 {
        let phone = phones.get(i as usize).map(|s| s.as_str()).unwrap_or("");
        let line  = format!("{:04},{}\r\n", i, phone);
        let (enc, _, _) = encoding_rs::SHIFT_JIS.encode(&line);
        writer.write_all(&enc)?;
    }
    writer.flush()?;

    Ok((path, count))
}


/// ナカヨ電話帳.csv形式で書き出す
///
/// 処理済みデータ（名前クリーニング・メモリ番号整列済み）を
/// ナカヨ電話機のインポート形式に変換して出力する
///
/// 出力列（電話帳.csvのベースファイルと同じ17列）:
///   短縮番号, 名前, ヨミガナ,
///   電話番号１, 外線グループ番号１, 番号種別１,
///   電話番号２, 外線グループ番号２, 番号種別２,
///   電話番号３, 外線グループ番号３, 番号種別３,
///   相手区分, マイ電話帳登録電話機, 表示モード, 定型文１, 定型文２
///
/// グループ9（迷惑電話）のデータは名前・電話番号がクリア済みのため
/// 空行として出力される（メモリ番号体系を維持するため）
fn write_nakayo_phonebook(
    df:        &CsvData,
    path:      &Path,
    mem_idx:   Option<usize>,
    name_idx:  Option<usize>,
    yomi_idx:  Option<usize>,
    phone_idx: Option<usize>,
    group_idx: Option<usize>,
) -> Result<usize, std::io::Error> {

    let file = fs::File::create(path)?;
    let mut writer = std::io::BufWriter::new(file);
    // UTF-8 BOM（ナカヨ管理ソフトはUTF-8 BOM付きを要求する場合がある）
    writer.write_all(&[0xEF, 0xBB, 0xBF])?;

    let mut csv_writer = csv::WriterBuilder::new().from_writer(writer);

    // ヘッダー（電話帳.csvと同じ列構成）
    csv_writer.write_record(&[
        "短縮番号", "名前", "ヨミガナ",
        "電話番号１", "外線グループ番号１", "番号種別１",
        "電話番号２", "外線グループ番号２", "番号種別２",
        "電話番号３", "外線グループ番号３", "番号種別３",
        "相手区分", "マイ電話帳登録電話機", "表示モード", "定型文１", "定型文２",
    ])?;

    let mut count = 0;
    for row in &df.rows {
        // ライフタイムエラー回避のためクロージャを使わずインライン展開
        let memory = mem_idx.and_then(|i| row.get(i)).map(|s| s.as_str()).unwrap_or("").to_string();
        let name   = name_idx.and_then(|i| row.get(i)).map(|s| s.as_str()).unwrap_or("").to_string();
        let yomi   = yomi_idx.and_then(|i| row.get(i)).map(|s| s.as_str()).unwrap_or("").to_string();
        let phone  = phone_idx.and_then(|i| row.get(i)).map(|s| s.as_str()).unwrap_or("").to_string();
        let group  = group_idx.and_then(|i| row.get(i)).map(|s| s.as_str()).unwrap_or("").to_string();

        // 外線グループ番号：グループ9は迷惑電話フラグ相当なのでデフォルト値に
        // それ以外はグループ番号をそのまま（ただし2桁ゼロ埋め）
        let ext_grp = if group == "9" || group.is_empty() {
            "00".to_string()
        } else {
            format!("{:0>2}", group)
        };

        csv_writer.write_record(&[
            &memory, &name, &yomi,
            &phone, &ext_grp, "0",  // 電話番号１・外線グループ番号１・番号種別１
            "", "00", "0",          // 電話番号２（処理済みCSVは1列のみ対応）
            "", "00", "0",          // 電話番号３
            "01", "000", "0", "000", "000",  // 相手区分等デフォルト値
        ])?;
        count += 1;
    }
    csv_writer.flush()?;
    Ok(count)
}

/// 処理統計情報の生成（Python: generate_statistics() に対応）
fn generate_statistics(df: &CsvData, original_path: &Path) -> Vec<String> {
    let mut stats = Vec::new();

    stats.push(format!(
        "処理日時: {}",
        Local::now().format("%Y/%m/%d %H:%M:%S")
    ));
    stats.push(format!(
        "元ファイル: {}",
        original_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("不明")
    ));
    stats.push(format!("総レコード数: {}行", df.row_count()));
    stats.push(format!("列数: {}列", df.col_count()));

    for (col_idx, col_name) in df.headers.iter().enumerate() {
        let non_empty = df.rows.iter()
            .filter(|r| r.get(col_idx).map(|v| !v.trim().is_empty()).unwrap_or(false))
            .count();
        stats.push(format!("  {}: {}件のデータ", col_name, non_empty));
    }

    stats
}
