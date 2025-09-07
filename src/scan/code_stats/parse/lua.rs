use crate::scan::code_stats::CodeParser;

#[derive(Debug, Default, Clone, Copy)]
pub struct Luaparser;

impl CodeParser for Luaparser {
    fn is_comment(&mut self, line: &str) -> bool {
        line.starts_with("--")
    }
}
