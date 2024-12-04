use std::fmt::{self, Display, Formatter};

pub const CODE_STDERR_REDIRECT: i32 = 10;
pub const CODE_PARSE_COMMAND_LINE_ARGS: i32 = 12;
pub const CODE_LOAD_CONFIG: i32 = 13;
pub const CODE_COMMAND_FAILED: i32 = 14;

pub const CODE_SILENT_EXIT: i32 = 100;

/// Custom error type for early exit.
#[derive(Debug)]
pub struct SilentExit {
    pub code: u8,
}

impl Display for SilentExit {
    fn fmt(&self, _: &mut Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}
