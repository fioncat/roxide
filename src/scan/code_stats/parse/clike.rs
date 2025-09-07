use crate::scan::code_stats::CodeParser;

#[derive(Debug, Default, Clone, Copy)]
pub struct ClikeParser {
    is_multi_line: bool,
}

impl CodeParser for ClikeParser {
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
            if line.ends_with("*/") {
                self.is_multi_line = true;
            }
            return true;
        }
        false
    }
}
