use anyhow::Result;

pub trait Run {
    fn run(&self) -> Result<()>;
}

pub trait Complete {
    fn complete(args: Vec<String>) -> Result<Vec<String>>;
}
