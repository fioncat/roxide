use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about)]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Home(HomeArgs),
    Complete(CompleteArgs),
}

#[derive(Args)]
pub struct HomeArgs {
    pub query: Vec<String>,

    #[clap(long, short)]
    pub search: bool,
}

#[derive(Args)]
pub struct CompleteArgs {
    pub args: Vec<String>,
}
