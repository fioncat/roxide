use clap::{Args, Parser};

#[derive(Parser)]
#[command(name = "example")]
struct App {
    #[command(flatten)]
    pub args: ExampleArgs,
}

#[derive(Args)]
struct ExampleArgs {
    /// 可选参数，支持三种状态：
    /// - 不传 -b: None
    /// - 传 -b: Some(None)  
    /// - 传 -b xxx: Some(Some("xxx"))
    #[arg(short = 'b', long = "branch", num_args = 0..=1)]
    pub branch: Option<Option<String>>,
}

fn main() {
    let app = App::parse();
    
    match app.args.branch {
        None => println!("没有传递 -b 参数"),
        Some(None) => println!("传递了 -b 但没有值"),
        Some(Some(value)) => println!("传递了 -b 并带有值: {}", value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_no_branch_arg() {
        let app = App::try_parse_from(&["example"]).unwrap();
        assert!(app.args.branch.is_none());
    }

    #[test]
    fn test_branch_without_value() {
        let app = App::try_parse_from(&["example", "-b"]).unwrap();
        assert_eq!(app.args.branch, Some(None));
    }

    #[test]
    fn test_branch_with_value() {
        let app = App::try_parse_from(&["example", "-b", "feature-branch"]).unwrap();
        assert_eq!(app.args.branch, Some(Some("feature-branch".to_string())));
    }

    #[test]
    fn test_branch_with_long_option() {
        let app = App::try_parse_from(&["example", "--branch", "main"]).unwrap();
        assert_eq!(app.args.branch, Some(Some("main".to_string())));
    }
}
