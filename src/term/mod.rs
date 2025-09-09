pub mod confirm;
pub mod list;
pub mod output;
pub mod table;
pub mod track;

use console::{Term, measure_text_width};

pub fn width() -> usize {
    let term = Term::stdout();
    let (_, width) = term.size();
    width as usize
}

pub fn truncate_by_width(text: &str, max_width: usize) -> String {
    const ELLIPSIS: &str = " ";

    if max_width == 0 {
        return String::new();
    }

    let current_width = measure_text_width(text);
    if current_width <= max_width {
        return text.to_string();
    }

    let ellipsis_width = measure_text_width(ELLIPSIS);
    if ellipsis_width >= max_width {
        return ".".repeat(max_width);
    }

    let target_width = max_width - ellipsis_width;
    let mut result = String::new();
    let mut current_width = 0;

    for ch in text.chars() {
        let char_width = measure_text_width(&ch.to_string());
        if current_width + char_width > target_width {
            break;
        }
        result.push(ch);
        current_width += char_width;
    }

    result.push_str(ELLIPSIS);
    result
}
