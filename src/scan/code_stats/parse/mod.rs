mod clike;
mod html;

use super::CodeParser;

macro_rules! generate_simple_parser {
    ($name:ident, $comment_prefix:expr) => {
        paste::paste! {
            #[derive(Debug, Default, Clone, Copy)]
            pub struct [<$name Parser>];

            impl CodeParser for [<$name Parser>] {
                fn is_comment(&mut self, line: &str) -> bool {
                    line.starts_with($comment_prefix)
                }
            }
        }
    };
}

generate_simple_parser!(LuaLike, "--");
generate_simple_parser!(Script, '#');
generate_simple_parser!(Semicolon, ';');
generate_simple_parser!(Percent, '%');
generate_simple_parser!(Quote, '"');

struct NoCommentParser;

impl CodeParser for NoCommentParser {
    fn is_comment(&mut self, _line: &str) -> bool {
        false
    }
}

pub fn new_parser(extension: &str) -> Option<(&'static str, Box<dyn CodeParser>)> {
    let (name, parser): (&'static str, Box<dyn CodeParser>) = match extension {
        // C-like
        "c" | "h" => ("C", Box::new(clike::CLikeParser::default())),
        "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hxx" => {
            ("C++", Box::new(clike::CLikeParser::default()))
        }
        "cs" => ("C#", Box::new(clike::CLikeParser::default())),
        "dart" => ("Dart", Box::new(clike::CLikeParser::default())),
        "go" => ("Go", Box::new(clike::CLikeParser::default())),
        "java" => ("Java", Box::new(clike::CLikeParser::default())),
        "js" | "ts" | "jsx" | "tsx" => (
            "JavaScript/TypeScript",
            Box::new(clike::CLikeParser::default()),
        ),
        "kt" => ("Kotlin", Box::new(clike::CLikeParser::default())),
        "nim" => ("Nim", Box::new(clike::CLikeParser::default())),
        "odin" => ("Odin", Box::new(clike::CLikeParser::default())),
        "php" => ("PHP", Box::new(clike::CLikeParser::default())),
        "rs" => ("Rust", Box::new(clike::CLikeParser::default())),
        "scala" => ("Scala", Box::new(clike::CLikeParser::default())),
        "swift" => ("Swift", Box::new(clike::CLikeParser::default())),
        "v" => ("V", Box::new(clike::CLikeParser::default())),
        "zig" => ("Zig", Box::new(clike::CLikeParser::default())),

        // Lua style
        "ada" | "adb" | "ads" => ("Ada", Box::new(LuaLikeParser)),
        "elm" => ("Elm", Box::new(LuaLikeParser)),
        "haskell" | "hs" => ("Haskell", Box::new(LuaLikeParser)),
        "idris" => ("Idris", Box::new(LuaLikeParser)),
        "lua" => ("Lua", Box::new(LuaLikeParser)),
        "sql" => ("SQL", Box::new(LuaLikeParser)),
        "vhdl" | "vhd" => ("VHDL", Box::new(LuaLikeParser)),

        // Shell style
        "awk" => ("AWK", Box::new(ScriptParser)),
        "bat" | "cmd" => ("Batch", Box::new(ScriptParser)),
        "cmake" => ("CMake", Box::new(ScriptParser)),
        "coffee" => ("CoffeeScript", Box::new(ScriptParser)),
        "crystal" => ("Crystal", Box::new(ScriptParser)),
        "dockerfile" => ("Dockerfile", Box::new(ScriptParser)),
        "fish" => ("Fish Shell", Box::new(ScriptParser)),
        "gradle" => ("Gradle", Box::new(ScriptParser)),
        "groovy" => ("Groovy", Box::new(ScriptParser)),
        "ini" | "cfg" => ("INI", Box::new(ScriptParser)),
        "jl" => ("Julia", Box::new(ScriptParser)),
        "makefile" | "mk" => ("Makefile", Box::new(ScriptParser)),
        "nix" => ("Nix", Box::new(ScriptParser)),
        "pl" | "pm" => ("Perl", Box::new(ScriptParser)),
        "properties" => ("Properties", Box::new(ScriptParser)),
        "ps1" => ("PowerShell", Box::new(ScriptParser)),
        "py" | "pyw" => ("Python", Box::new(ScriptParser)),
        "r" => ("R", Box::new(ScriptParser)),
        "rb" => ("Ruby", Box::new(ScriptParser)),
        "sh" | "zsh" | "bash" => ("Shell", Box::new(ScriptParser)),
        "tcl" => ("Tcl", Box::new(ScriptParser)),
        "tf" | "hcl" => ("Terraform", Box::new(ScriptParser)),
        "toml" => ("TOML", Box::new(ScriptParser)),
        "yaml" | "yml" => ("YAML", Box::new(ScriptParser)),

        // HTML style
        "html" | "htm" => ("HTML", Box::new(html::HtmlParser::default())),
        "xml" => ("XML", Box::new(html::HtmlParser::default())),
        "xhtml" => ("XHTML", Box::new(html::HtmlParser::default())),
        "svg" => ("SVG", Box::new(html::HtmlParser::default())),

        "lisp" | "lsp" => ("Lisp", Box::new(SemicolonParser)),
        "clj" | "cljs" | "cljc" => ("Clojure", Box::new(SemicolonParser)),
        "scm" => ("Scheme", Box::new(SemicolonParser)),
        "rkt" => ("Racket", Box::new(SemicolonParser)),
        "el" => ("Emacs Lisp", Box::new(SemicolonParser)),
        "asm" | "s" | "S" => ("Assembly", Box::new(SemicolonParser)),
        "nasm" => ("NASM", Box::new(SemicolonParser)),
        "masm" => ("MASM", Box::new(SemicolonParser)),

        "tex" | "latex" => ("LaTeX", Box::new(PercentParser)),
        "erl" | "hrl" => ("Erlang", Box::new(PercentParser)),
        "ex" | "exs" => ("Elixir", Box::new(PercentParser)),
        "m" => ("MATLAB", Box::new(PercentParser)),
        "pro" => ("Prolog", Box::new(PercentParser)),
        "plt" => ("Gnuplot", Box::new(PercentParser)),

        "vim" | "vimrc" => ("Vim Script", Box::new(QuoteParser)),
        "vb" | "vbs" => ("Visual Basic", Box::new(QuoteParser)),
        "bas" => ("BASIC", Box::new(QuoteParser)),
        "cls" => ("Visual Basic Class", Box::new(QuoteParser)),

        "json" => ("JSON", Box::new(NoCommentParser)),
        "csv" => ("CSV", Box::new(NoCommentParser)),
        "tsv" => ("TSV", Box::new(NoCommentParser)),
        "md" | "markdown" => ("Markdown", Box::new(NoCommentParser)),

        _ => return None,
    };
    Some((name, parser))
}

pub fn get_special_file(name: &str) -> Option<&'static str> {
    match name {
        "Makefile" | "makefile" | "GNUmakefile" => Some("makefile"),
        "CMakeLists.txt" => Some("cmake"),
        "build.gradle" | "settings.gradle" => Some("gradle"),
        "build.gradle.kts" | "settings.gradle.kts" => Some("gradle"),

        "Dockerfile" | "dockerfile" => Some("dockerfile"),

        ".vimrc" | ".gvimrc" => Some("vim"),

        ".emacs" | ".spacemacs" => Some("el"),

        _ => None,
    }
}
