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

pub fn new_parser(extension: &str) -> Option<(&'static str, bool, Box<dyn CodeParser>)> {
    let (name, main, parser): (&'static str, bool, Box<dyn CodeParser>) = match extension {
        // C-like
        "c" => ("C", true, Box::new(clike::CLikeParser::default())),
        "cpp" | "cc" | "cxx" | "c++" => ("C++", true, Box::new(clike::CLikeParser::default())),
        "cs" => ("C#", true, Box::new(clike::CLikeParser::default())),
        "dart" => ("Dart", true, Box::new(clike::CLikeParser::default())),
        "go" => ("Go", true, Box::new(clike::CLikeParser::default())),
        "java" => ("Java", true, Box::new(clike::CLikeParser::default())),
        "js" | "jsx" => ("JavaScript", true, Box::new(clike::CLikeParser::default())),
        "kt" => ("Kotlin", true, Box::new(clike::CLikeParser::default())),
        "nim" => ("Nim", true, Box::new(clike::CLikeParser::default())),
        "odin" => ("Odin", true, Box::new(clike::CLikeParser::default())),
        "php" => ("PHP", true, Box::new(clike::CLikeParser::default())),
        "rs" => ("Rust", true, Box::new(clike::CLikeParser::default())),
        "scala" => ("Scala", true, Box::new(clike::CLikeParser::default())),
        "swift" => ("Swift", true, Box::new(clike::CLikeParser::default())),
        "ts" | "tsx" => ("TypeScript", true, Box::new(clike::CLikeParser::default())),
        "v" => ("V", true, Box::new(clike::CLikeParser::default())),
        "zig" => ("Zig", true, Box::new(clike::CLikeParser::default())),

        "h" | "hpp" | "hxx" => (
            "C/C++ Header",
            false,
            Box::new(clike::CLikeParser::default()),
        ),

        // Lua style
        "ada" | "adb" | "ads" => ("Ada", true, Box::new(LuaLikeParser)),
        "elm" => ("Elm", true, Box::new(LuaLikeParser)),
        "haskell" | "hs" => ("Haskell", true, Box::new(LuaLikeParser)),
        "idris" => ("Idris", true, Box::new(LuaLikeParser)),
        "lua" => ("Lua", true, Box::new(LuaLikeParser)),
        "vhdl" | "vhd" => ("VHDL", true, Box::new(LuaLikeParser)),

        "sql" => ("SQL", false, Box::new(LuaLikeParser)),

        // Shell style
        "fish" => ("Fish Shell", true, Box::new(ScriptParser)),
        "jl" => ("Julia", true, Box::new(ScriptParser)),
        "ps1" => ("PowerShell", true, Box::new(ScriptParser)),
        "py" | "pyw" => ("Python", true, Box::new(ScriptParser)),
        "r" => ("R", true, Box::new(ScriptParser)),
        "rb" => ("Ruby", true, Box::new(ScriptParser)),
        "sh" | "zsh" | "bash" => ("Shell", true, Box::new(ScriptParser)),

        "awk" => ("AWK", false, Box::new(ScriptParser)),
        "bat" | "cmd" => ("Batch", false, Box::new(ScriptParser)),
        "cmake" => ("CMake", false, Box::new(ScriptParser)),
        "coffee" => ("CoffeeScript", false, Box::new(ScriptParser)),
        "crystal" => ("Crystal", false, Box::new(ScriptParser)),
        "dockerfile" => ("Dockerfile", false, Box::new(ScriptParser)),
        "gradle" => ("Gradle", false, Box::new(ScriptParser)),
        "groovy" => ("Groovy", false, Box::new(ScriptParser)),
        "ini" | "cfg" => ("INI", false, Box::new(ScriptParser)),
        "makefile" | "mk" => ("Makefile", false, Box::new(ScriptParser)),
        "nix" => ("Nix", false, Box::new(ScriptParser)),
        "pl" | "pm" => ("Perl", false, Box::new(ScriptParser)),
        "properties" => ("Properties", false, Box::new(ScriptParser)),
        "tcl" => ("Tcl", false, Box::new(ScriptParser)),
        "tf" | "hcl" => ("Terraform", false, Box::new(ScriptParser)),
        "toml" => ("TOML", false, Box::new(ScriptParser)),
        "yaml" | "yml" => ("YAML", false, Box::new(ScriptParser)),

        // HTML style
        "html" | "htm" => ("HTML", false, Box::new(html::HtmlParser::default())),
        "xml" => ("XML", false, Box::new(html::HtmlParser::default())),
        "xhtml" => ("XHTML", false, Box::new(html::HtmlParser::default())),
        "svg" => ("SVG", false, Box::new(html::HtmlParser::default())),

        // Lisp style
        "lisp" | "lsp" => ("Lisp", true, Box::new(SemicolonParser)),

        "asm" | "s" | "S" => ("Assembly", false, Box::new(SemicolonParser)),
        "clj" | "cljs" | "cljc" => ("Clojure", false, Box::new(SemicolonParser)),
        "el" => ("Emacs Lisp", true, Box::new(SemicolonParser)),
        "masm" => ("MASM", false, Box::new(SemicolonParser)),
        "nasm" => ("NASM", false, Box::new(SemicolonParser)),
        "rkt" => ("Racket", false, Box::new(SemicolonParser)),
        "scm" => ("Scheme", false, Box::new(SemicolonParser)),

        // Erlang style
        "erl" | "hrl" => ("Erlang", true, Box::new(PercentParser)),
        "m" => ("MATLAB", true, Box::new(PercentParser)),

        "ex" | "exs" => ("Elixir", false, Box::new(PercentParser)),
        "plt" => ("Gnuplot", false, Box::new(PercentParser)),
        "pro" => ("Prolog", false, Box::new(PercentParser)),
        "tex" | "latex" => ("LaTeX", false, Box::new(PercentParser)),

        // VimScript style
        "bas" => ("BASIC", true, Box::new(QuoteParser)),
        "vb" | "vbs" => ("Visual Basic", true, Box::new(QuoteParser)),

        "cls" => ("Visual Basic Class", false, Box::new(QuoteParser)),
        "vim" | "vimrc" => ("Vim Script", false, Box::new(QuoteParser)),

        // No comment
        "csv" => ("CSV", false, Box::new(NoCommentParser)),
        "json" => ("JSON", false, Box::new(NoCommentParser)),
        "md" | "markdown" => ("Markdown", false, Box::new(NoCommentParser)),
        "tsv" => ("TSV", false, Box::new(NoCommentParser)),

        _ => return None,
    };
    Some((name, main, parser))
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
