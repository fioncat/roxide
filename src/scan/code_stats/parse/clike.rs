use crate::scan::code_stats::CodeParser;

#[derive(Debug, Default, Clone, Copy)]
pub struct CLikeParser {
    is_multi_line: bool,
}

impl CodeParser for CLikeParser {
    fn is_comment(&mut self, line: &str) -> bool {
        if self.is_multi_line {
            if line.ends_with("*/") {
                self.is_multi_line = false;
            }
            return true;
        }

        if line.starts_with("//") {
            return true;
        }

        if line.starts_with("/*") {
            if !line.ends_with("*/") {
                // Inline comment, like `/* comment */ int a = 32;`
                if line.contains("*/") {
                    return false;
                }
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
    fn test_clike_parser() {
        #[derive(Debug)]
        struct Case {
            lines: Vec<&'static str>,
            expect: usize,
        }

        let cases = [
            Case {
                lines: vec![
                    "// This is a single line comment",
                    "int main() {",
                    "// This is another comment",
                    "   return 0;",
                    "}",
                ],
                expect: 2,
            },
            Case {
                lines: vec![
                    "/** This is a multi-line comment",
                    "  * Another line of comment",
                    "  * End of comment */",
                    "int main() {",
                    "   return 0;",
                    "}",
                ],
                expect: 3,
            },
            Case {
                lines: vec![
                    "/* Start of multi-line comment",
                    "   This is inside the comment",
                    "   Still inside the comment",
                    "   End of comment */",
                    "int x = 5;",
                    "// Single line comment",
                ],
                expect: 5,
            },
            Case {
                lines: vec![
                    "// Single line comment",
                    "/* Inline comment */ int y = 10;",
                    "/* Start multi-line",
                    "   continue multi-line */",
                    "// Another single line",
                ],
                expect: 4,
            },
            Case {
                lines: vec!["//", "/**/", "", "/* */", "// "],
                expect: 4,
            },
            Case {
                lines: vec![
                    "printf(\"This // is not a comment\");",
                    "// This is a real comment",
                    "char* str = \"/* not a comment */\";",
                    "/* Real comment */",
                ],
                expect: 2,
            },
            Case {
                lines: vec![
                    "/* First comment */",
                    "/* Second comment",
                    "   continues here */",
                    "int main() {",
                    "/* Another comment",
                    "   that spans",
                    "   multiple lines */",
                    "}",
                ],
                expect: 6,
            },
        ];

        for case in cases {
            let mut parser = CLikeParser::default();
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
