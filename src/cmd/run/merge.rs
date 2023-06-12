use clap::Args;

/// Create or open PullRequest (MeregeRequest for Gitlab)
#[derive(Args)]
pub struct MergeArgs {
    /// Upstream mode, only used for forked repo
    #[clap(long, short)]
    pub upstream: bool,

    /// Source branch, default will use current branch
    #[clap(long, short)]
    pub source: Option<String>,

    /// Target branch, default will use HEAD branch
    #[clap(long, short)]
    pub target: Option<String>,
}
