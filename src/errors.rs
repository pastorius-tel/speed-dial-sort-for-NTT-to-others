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
// errors.rs - カスタム例外クラス（Python版に対応）
// ======================================================

use thiserror::Error;

/// CSV処理に関するカスタムエラー（Python: CSVProcessingError）
#[derive(Debug, Error)]
pub enum CsvProcessingError {
    #[error("CSVファイルが空です")]
    EmptyFile,

    #[error("CSV形式が不正です: {0}")]
    InvalidFormat(String),

    #[error("ファイル保存に失敗しました: {0}")]
    SaveFailed(String),

    #[error("CSV読み込みエラー: {0}")]
    ReadError(String),
}

/// ファイルエンコーディングに関するカスタムエラー（Python: FileEncodingError）
#[derive(Debug, Error)]
pub enum FileEncodingError {
    #[error("ファイルが空です")]
    EmptyFile,

    #[error("エンコーディング検出に失敗しました: {0}")]
    DetectionFailed(String),
}

/// アプリケーション全体のエラー型
#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Csv(#[from] CsvProcessingError),

    #[error("{0}")]
    Encoding(#[from] FileEncodingError),

    #[error("I/Oエラー: {0}")]
    Io(#[from] std::io::Error),
}
