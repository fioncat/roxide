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

const SEP: &str = ", ";
const OMIT: &str = ", ...";

const SEP_WIDTH: usize = SEP.len();
const OMIT_WIDTH: usize = OMIT.len();

pub fn render_list<I, S>(items: I, count: usize, width: usize) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut list = String::with_capacity(width);
    for (idx, item) in items.into_iter().enumerate() {
        let add_width = if idx == 0 {
            console::measure_text_width(item.as_ref())
        } else {
            console::measure_text_width(item.as_ref()) + SEP_WIDTH
        };
        let is_last = idx == count - 1;
        let list_width = console::measure_text_width(&list);
        let new_size = list_width + add_width;
        if new_size > width || (!is_last && new_size == width) {
            let delta = width - list_width;
            if delta == 0 {
                break;
            }
            if delta < OMIT_WIDTH {
                list.push_str(&".".repeat(delta));
            } else {
                list.push_str(OMIT);
            }
            break;
        }
        if idx != 0 {
            list.push_str(SEP);
        }
        list.push_str(item.as_ref());
    }
    list
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

    #[test]
    fn test_render_list() {
        #[derive(Debug)]
        struct Case {
            items: Vec<&'static str>,
            width: usize,
            expect: &'static str,
        }

        let cases = vec![
            Case {
                items: vec!["apple", "banana", "cherry"],
                width: 20,
                expect: "apple, banana, ...",
            },
            Case {
                items: vec!["apple", "banana", "cherry"],
                width: 15,
                expect: "apple, banana..",
            },
            Case {
                items: vec!["apple", "banana", "cherry"],
                width: 10,
                expect: "apple, ...",
            },
            Case {
                items: vec!["apple", "banana", "cherry"],
                width: 3,
                expect: "...",
            },
            Case {
                items: vec!["apple", "banana", "cherry"],
                width: 0,
                expect: "",
            },
            Case {
                items: vec!["a", "b", "c", "d", "e"],
                width: 9,
                expect: "a, b, c..",
            },
            Case {
                items: vec!["a", "b", "c", "d", "e"],
                width: 7,
                expect: "a, b...",
            },
            Case {
                items: vec!["a", "b", "c", "d", "e"],
                width: 5,
                expect: "a, b.",
            },
        ];

        for case in cases {
            let result = render_list(&case.items, case.items.len(), case.width);
            assert_eq!(result, case.expect, "Failed for {case:?}");
        }
    }
}
