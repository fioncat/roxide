use crate::scan::code_stats::CodeParser;

#[derive(Debug, Default, Clone, Copy)]
pub struct HtmlParser {
    is_multi_line: bool,
}

impl CodeParser for HtmlParser {
    fn is_comment(&mut self, line: &str) -> bool {
        if self.is_multi_line {
            if line.contains("-->") {
                self.is_multi_line = false;
            }
            return true;
        }

        if line.trim_start().starts_with("<!--") {
            if !line.contains("-->") {
                self.is_multi_line = true;
            }
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_parser() {
        #[derive(Debug)]
        struct Case {
            lines: Vec<&'static str>,
            expect: usize,
        }

        let cases = [
            Case {
                lines: vec![
                    "<!-- Single line HTML comment -->",
                    "<div>content</div>",
                    "<!-- Another comment -->",
                ],
                expect: 2,
            },
            Case {
                lines: vec![
                    "<!-- Multi-line comment start",
                    "     Comment content",
                    "     Comment end -->",
                    "<p>Normal content</p>",
                ],
                expect: 3,
            },
        ];

        for case in cases {
            let mut parser = HtmlParser::default();
            let mut comment_count = 0;
            for line in case.lines.iter() {
                if parser.is_comment(line) {
                    comment_count += 1;
                }
            }
            assert_eq!(comment_count, case.expect, "{case:?}");
        }
    }
} 