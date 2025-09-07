use crate::scan::code_stats::CodeParser;

#[derive(Debug, Default, Clone, Copy)]
pub struct ScriptParser;

impl CodeParser for ScriptParser {
    fn is_comment(&mut self, line: &str) -> bool {
        line.starts_with('#')
    }
}
