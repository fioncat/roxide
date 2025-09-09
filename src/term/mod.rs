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
    if max_width == 0 {
        return String::new();
    }

    let current_width = measure_text_width(text);
    if current_width <= max_width {
        return text.to_string();
    }

    let mut result = String::new();
    let mut current_width = 0;
    for ch in text.chars() {
        let char_width = measure_text_width(&ch.to_string());
        if current_width + char_width > max_width {
            break;
        }
        result.push(ch);
        current_width += char_width;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_by_width() {
        assert_eq!(truncate_by_width("Hello, World!", 5), "Hello");
        assert_eq!(truncate_by_width("Hello", 10), "Hello");
        assert_eq!(truncate_by_width("世界你好", 5), "世界");
        assert_eq!(truncate_by_width("こんにちは", 6), "こんに");
        assert_eq!(truncate_by_width("A", 0), "");
        assert_eq!(truncate_by_width("A", 1), "A");
        assert_eq!(truncate_by_width("A", 2), "A");
    }
}
