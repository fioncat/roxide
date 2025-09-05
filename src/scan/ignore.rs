use std::path::Path;

use anyhow::{Context, Result};
use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

#[derive(Debug, Clone, Default)]
pub struct Ignore {
    ignore: Option<Gitignore>,
}

impl Ignore {
    pub fn parse<P, I, S>(path: P, lines: I) -> Result<Self>
    where
        P: AsRef<Path>,
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut builder = GitignoreBuilder::new(path.as_ref());
        for line in lines {
            builder
                .add_line(None, line.as_ref())
                .with_context(|| format!("failed to parse ignore pattern {:?}", line.as_ref()))?;
        }

        let ignore = builder.build().with_context(|| {
            format!(
                "failed to build ignore matcher for path {:?}",
                path.as_ref().display()
            )
        })?;

        Ok(Self {
            ignore: Some(ignore),
        })
    }

    pub fn matched<P>(&self, path: P, is_dir: bool) -> bool
    where
        P: AsRef<Path>,
    {
        let Some(ref ignore) = self.ignore else {
            return false;
        };
        let matched = ignore.matched(path, is_dir);
        !matches!(matched, Match::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ignore() {
        let ignore = Ignore::parse(
            ".",
            ["test/**", "src/**/*.go", "pkg/*.go", ".git", "target"],
        )
        .unwrap();

        let cases = [
            ("test/hello.go", true),
            ("test/main.rs", true),
            ("test/sub/main.rs", true),
            ("src/test/main.go", true),
            ("src/main.rs", false),
            ("src/simple", false),
            ("src/.git", true),
            ("pkg/time", false),
            ("pkg/time/time.go", false),
            ("pkg/sample.go", true),
            ("pkg/.git", true),
            (".git", true),
            ("config/.git", true),
            ("target", true),
            ("pkg/target", true),
        ];
        for (path, expect) in cases {
            let is_dir = path.ends_with(".go") || path.ends_with(".rs");
            let matched = ignore.matched(path, is_dir);
            assert_eq!(matched, expect, "Path {path:?} unexpected");
        }
    }
}
