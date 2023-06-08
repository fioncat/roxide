mod app;
mod complete;
mod run;

pub use app::App;

use anyhow::Result;

pub trait Run {
    fn run(&self) -> Result<()>;
}
