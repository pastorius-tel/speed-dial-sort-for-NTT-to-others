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
// text_cleaner.rs - テキストクリーニング処理
// ======================================================

use log::warn;
use regex::Regex;
use std::sync::OnceLock;
use unicode_normalization::UnicodeNormalization;

// ── 正規表現キャッシュ（修正#4・#9: Regex::new を毎回呼ばないよう OnceLock でキャッシュ）──
static MULTI_SPACE_RE: OnceLock<Regex> = OnceLock::new();
static WHITESPACE_RE:  OnceLock<Regex> = OnceLock::new();

fn multi_space_re() -> &'static Regex {
    MULTI_SPACE_RE.get_or_init(|| Regex::new(r"[ \u3000]+").unwrap())
}

fn whitespace_re() -> &'static Regex {
    WHITESPACE_RE.get_or_init(|| Regex::new(r"\s+").unwrap())
}

/// Unicode全角→半角変換
/// 修正#5: 濁音・半濁音を正しく2文字（基底文字+結合文字）に変換
fn z2h(text: &str) -> String {
    let mut result = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        let cp = c as u32;
        match cp {
            // 全角ASCII (！～) → 半角
            0xFF01..=0xFF5E => {
                result.push(char::from_u32(cp - 0xFEE0).unwrap_or(c));
            }
            // 全角スペース → 半角スペース
            0x3000 => result.push(' '),

            // ── 全角カタカナ → 半角カタカナ（修正#5: 濁点・半濁点を別文字で付加）──
            // 清音
            0x30A1 => result.push('\u{FF67}'), // ァ
            0x30A2 => result.push('\u{FF71}'), // ア
            0x30A3 => result.push('\u{FF68}'), // ィ
            0x30A4 => result.push('\u{FF72}'), // イ
            0x30A5 => result.push('\u{FF69}'), // ゥ
            0x30A6 => result.push('\u{FF73}'), // ウ
            0x30A7 => result.push('\u{FF6A}'), // ェ
            0x30A8 => result.push('\u{FF74}'), // エ
            0x30A9 => result.push('\u{FF6B}'), // ォ
            0x30AA => result.push('\u{FF75}'), // オ
            0x30AB => result.push('\u{FF76}'), // カ
            0x30AD => result.push('\u{FF77}'), // キ
            0x30AF => result.push('\u{FF78}'), // ク
            0x30B1 => result.push('\u{FF79}'), // ケ
            0x30B3 => result.push('\u{FF7A}'), // コ
            0x30B5 => result.push('\u{FF7B}'), // サ
            0x30B7 => result.push('\u{FF7C}'), // シ
            0x30B9 => result.push('\u{FF7D}'), // ス
            0x30BB => result.push('\u{FF7E}'), // セ
            0x30BD => result.push('\u{FF7F}'), // ソ
            0x30BF => result.push('\u{FF80}'), // タ
            0x30C1 => result.push('\u{FF81}'), // チ
            0x30C3 => result.push('\u{FF6F}'), // ッ
            0x30C4 => result.push('\u{FF82}'), // ツ
            0x30C6 => result.push('\u{FF83}'), // テ
            0x30C8 => result.push('\u{FF84}'), // ト
            0x30CA => result.push('\u{FF85}'), // ナ
            0x30CB => result.push('\u{FF86}'), // ニ
            0x30CC => result.push('\u{FF87}'), // ヌ
            0x30CD => result.push('\u{FF88}'), // ネ
            0x30CE => result.push('\u{FF89}'), // ノ
            0x30CF => result.push('\u{FF8A}'), // ハ
            0x30D2 => result.push('\u{FF8B}'), // ヒ
            0x30D5 => result.push('\u{FF8C}'), // フ
            0x30D8 => result.push('\u{FF8D}'), // ヘ
            0x30DB => result.push('\u{FF8E}'), // ホ
            0x30DE => result.push('\u{FF8F}'), // マ
            0x30DF => result.push('\u{FF90}'), // ミ
            0x30E0 => result.push('\u{FF91}'), // ム
            0x30E1 => result.push('\u{FF92}'), // メ
            0x30E2 => result.push('\u{FF93}'), // モ
            0x30E3 => result.push('\u{FF6C}'), // ャ
            0x30E4 => result.push('\u{FF94}'), // ヤ
            0x30E5 => result.push('\u{FF6D}'), // ュ
            0x30E6 => result.push('\u{FF95}'), // ユ
            0x30E7 => result.push('\u{FF6E}'), // ョ
            0x30E8 => result.push('\u{FF96}'), // ヨ
            0x30E9 => result.push('\u{FF97}'), // ラ
            0x30EA => result.push('\u{FF98}'), // リ
            0x30EB => result.push('\u{FF99}'), // ル
            0x30EC => result.push('\u{FF9A}'), // レ
            0x30ED => result.push('\u{FF9B}'), // ロ
            0x30EF => result.push('\u{FF9C}'), // ワ
            0x30F2 => result.push('\u{FF66}'), // ヲ
            0x30F3 => result.push('\u{FF9D}'), // ン
            0x30FB => result.push('\u{FF65}'), // ・
            0x30FC => result.push('\u{FF70}'), // ー

            // 濁音（基底文字 + 半角濁点 ﾞ U+FF9E）
            0x30AC => { result.push('\u{FF76}'); result.push('\u{FF9E}'); } // ガ
            0x30AE => { result.push('\u{FF77}'); result.push('\u{FF9E}'); } // ギ
            0x30B0 => { result.push('\u{FF78}'); result.push('\u{FF9E}'); } // グ
            0x30B2 => { result.push('\u{FF79}'); result.push('\u{FF9E}'); } // ゲ
            0x30B4 => { result.push('\u{FF7A}'); result.push('\u{FF9E}'); } // ゴ
            0x30B6 => { result.push('\u{FF7B}'); result.push('\u{FF9E}'); } // ザ
            0x30B8 => { result.push('\u{FF7C}'); result.push('\u{FF9E}'); } // ジ
            0x30BA => { result.push('\u{FF7D}'); result.push('\u{FF9E}'); } // ズ
            0x30BC => { result.push('\u{FF7E}'); result.push('\u{FF9E}'); } // ゼ
            0x30BE => { result.push('\u{FF7F}'); result.push('\u{FF9E}'); } // ゾ
            0x30C0 => { result.push('\u{FF80}'); result.push('\u{FF9E}'); } // ダ
            0x30C2 => { result.push('\u{FF81}'); result.push('\u{FF9E}'); } // ヂ
            0x30C5 => { result.push('\u{FF82}'); result.push('\u{FF9E}'); } // ヅ
            0x30C7 => { result.push('\u{FF83}'); result.push('\u{FF9E}'); } // デ
            0x30C9 => { result.push('\u{FF84}'); result.push('\u{FF9E}'); } // ド
            0x30D0 => { result.push('\u{FF8A}'); result.push('\u{FF9E}'); } // バ
            0x30D3 => { result.push('\u{FF8B}'); result.push('\u{FF9E}'); } // ビ
            0x30D6 => { result.push('\u{FF8C}'); result.push('\u{FF9E}'); } // ブ
            0x30D9 => { result.push('\u{FF8D}'); result.push('\u{FF9E}'); } // ベ
            0x30DC => { result.push('\u{FF8E}'); result.push('\u{FF9E}'); } // ボ
            0x30F4 => { result.push('\u{FF73}'); result.push('\u{FF9E}'); } // ヴ

            // 半濁音（基底文字 + 半角半濁点 ﾟ U+FF9F）
            0x30D1 => { result.push('\u{FF8A}'); result.push('\u{FF9F}'); } // パ
            0x30D4 => { result.push('\u{FF8B}'); result.push('\u{FF9F}'); } // ピ
            0x30D7 => { result.push('\u{FF8C}'); result.push('\u{FF9F}'); } // プ
            0x30DA => { result.push('\u{FF8D}'); result.push('\u{FF9F}'); } // ペ
            0x30DD => { result.push('\u{FF8E}'); result.push('\u{FF9F}'); } // ポ

            // その他（近似変換）
            0x30EE => result.push('\u{FF9C}'), // ヮ → ワ に近似
            0x30F0 => result.push('\u{FF72}'), // ヰ → イ
            0x30F1 => result.push('\u{FF74}'), // ヱ → エ

            _ => result.push(c),
        }
    }
    result
}

/// テキストのクリーニング処理
pub fn clean_text(text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    // 1. Unicode正規化 NFKC
    let normalized: String = text.nfkc().collect();
    // 2. 全角→半角変換
    let half_width = z2h(&normalized);
    // 3. 複数の空白文字を単一スペースに統一
    multi_space_re().replace_all(&half_width, " ").trim().to_string()
}

/// 会社名のクリーニング処理
pub fn clean_company_names(
    text: &str,
    remove_terms: &[String],
    remove_spaces: bool,
) -> String {
    if text.trim().is_empty() {
        return String::new();
    }

    let mut result = clean_text(text);

    for term in remove_terms {
        let term = term.trim();
        if term.is_empty() {
            continue;
        }
        let escaped = regex::escape(term);
        let pattern_str = format!(r"(?i)\s*[\(\[]?\s*{}\s*[\)\]]?\s*", escaped);
        match Regex::new(&pattern_str) {
            Ok(re) => result = re.replace_all(&result, "").to_string(),
            Err(e) => warn!("正規表現エラー (term='{}'): {}", term, e),
        }
    }

    // 修正#4・#9: キャッシュ済み正規表現を使用
    result = whitespace_re().replace_all(&result, " ").trim().to_string();

    if remove_spaces {
        result = result.replace(' ', "");
    }
    result
}

/// 電話番号の # 付加/削除処理
pub fn process_phone_sharp(phone: &str, add_sharp: bool) -> String {
    let phone = phone.trim();
    if phone.is_empty() {
        return String::new();
    }
    if add_sharp {
        if phone.ends_with('#') { phone.to_string() } else { format!("{}#", phone) }
    } else {
        if phone.ends_with('#') { phone[..phone.len() - 1].to_string() } else { phone.to_string() }
    }
}

// ── 文字幅カウント・文字数制限チェック ──

/// 文字が半角かどうかを判定
pub fn is_halfwidth(c: char) -> bool {
    let cp = c as u32;
    matches!(cp,
        0x0020..=0x007E
        | 0xFF61..=0xFF9F // 半角カタカナ・記号
    )
}

/// 文字列の表示幅を計算（全角=2、半角=1）
pub fn display_width(text: &str) -> usize {
    text.chars().map(|c| if is_halfwidth(c) { 1 } else { 2 }).sum()
}

/// 文字数オーバーの詳細情報
pub struct NameLengthInfo {
    pub width: usize,
    pub over: bool,
    pub over_by: usize,
}

/// 名前の文字数制限チェック
pub fn check_name_length(text: &str, limit: usize) -> NameLengthInfo {
    let width = display_width(text);
    let over = width > limit;
    let over_by = if over { width - limit } else { 0 };
    NameLengthInfo { width, over, over_by }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_text_basic() {
        assert_eq!(clean_text("  hello  "), "hello");
        assert_eq!(clean_text(""), "");
        assert_eq!(clean_text("　　"), "");
    }

    #[test]
    fn test_clean_company_names() {
        let terms = vec!["株式会社".to_string(), "有限会社".to_string()];
        assert_eq!(clean_company_names("株式会社テスト", &terms, false), "テスト");
        assert_eq!(clean_company_names("テスト有限会社", &terms, false), "テスト");
        assert_eq!(clean_company_names("(株)テスト", &terms, false), "テスト");
    }

    #[test]
    fn test_z2h_dakuten() {
        // 修正#5: 濁音・半濁音が正しく変換されることを確認
        let result = z2h("ガギグゲゴ");
        assert!(result.contains('\u{FF9E}'), "濁点が含まれること: {}", result);
        let result2 = z2h("パピプペポ");
        assert!(result2.contains('\u{FF9F}'), "半濁点が含まれること: {}", result2);
        // ガ → ｶﾞ (2文字)
        let ga = z2h("ガ");
        assert_eq!(ga.chars().count(), 2);
    }

    #[test]
    fn test_process_phone_sharp() {
        assert_eq!(process_phone_sharp("0312345678", true), "0312345678#");
        assert_eq!(process_phone_sharp("0312345678#", true), "0312345678#");
        assert_eq!(process_phone_sharp("0312345678#", false), "0312345678");
        assert_eq!(process_phone_sharp("0312345678", false), "0312345678");
        assert_eq!(process_phone_sharp("", true), "");
    }

    #[test]
    fn test_display_width() {
        assert_eq!(display_width("山田太郎"),    8);
        assert_eq!(display_width("ﾔﾏﾀﾞﾀﾛｳ"),  7);
        assert_eq!(display_width("YAMADA"),      6);
        assert_eq!(display_width("山田太郎花子"),12);
    }

    #[test]
    fn test_check_name_length_16() {
        assert!(!check_name_length("山田太郎花子桃子", 16).over);
        assert!( check_name_length("山田太郎花子桃子a", 16).over);
        assert!(!check_name_length("YAMADA TARO 123 ", 16).over);
    }

    #[test]
    fn test_check_name_length_20() {
        assert!(!check_name_length("山田太郎花子桃子次郎", 20).over);
        assert!( check_name_length("山田太郎花子桃子次郎a", 20).over);
        assert!(!check_name_length("YAMADA TARO 12345678", 20).over);
    }
}
