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
// encoding.rs - ファイルエンコーディング検出
// Python版の detect_encoding() に対応
// ======================================================

use crate::errors::FileEncodingError;
use chardetng::EncodingDetector;
use log::{info, warn};
use std::fs;
use std::path::Path;

/// ファイルの文字エンコーディングを検出する
/// Python版の detect_encoding() に対応
pub fn detect_encoding(file_path: &Path) -> Result<&'static encoding_rs::Encoding, FileEncodingError> {
    let raw_data = fs::read(file_path).map_err(|e| {
        FileEncodingError::DetectionFailed(e.to_string())
    })?;

    if raw_data.is_empty() {
        return Err(FileEncodingError::EmptyFile);
    }

    // BOM チェック（UTF-8 BOM）
    if raw_data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        info!("UTF-8 BOM を検出");
        return Ok(encoding_rs::UTF_8);
    }

    // chardetng でエンコーディング検出
    let mut detector = EncodingDetector::new();
    detector.feed(&raw_data, true);
    let encoding = detector.guess(None, true);

    info!("検出されたエンコーディング: {}", encoding.name());

    // 主要な日本語エンコーディングを試行（Python版のフォールバック処理に対応）
    let is_low_confidence = matches!(encoding.name(), "windows-1252" | "ISO-8859-1");
    if is_low_confidence {
        warn!("エンコーディング検出の信頼度が低いため、フォールバックを試みます");
        let fallbacks: &[&'static encoding_rs::Encoding] = &[
            encoding_rs::UTF_8,
            encoding_rs::SHIFT_JIS,
            encoding_rs::EUC_JP,
            encoding_rs::ISO_2022_JP,
        ];
        for &enc in fallbacks {
            let (_, _, had_errors) = enc.decode(&raw_data);
            if !had_errors {
                info!("フォールバックエンコーディングを使用: {}", enc.name());
                return Ok(enc);
            }
        }
    }

    Ok(encoding)
}
