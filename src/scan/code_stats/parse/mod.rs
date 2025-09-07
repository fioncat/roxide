mod clike;
mod lua;
mod script;

use super::CodeParser;

pub fn new_parser(extension: &str) -> Option<(&'static str, Box<dyn CodeParser>)> {
    let (name, parser): (&'static str, Box<dyn CodeParser>) = match extension {
        // C-like
        "c" | "h" => ("C", Box::new(clike::ClikeParser::default())),
        "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hxx" => {
            ("C++", Box::new(clike::ClikeParser::default()))
        }
        "cs" => ("C#", Box::new(clike::ClikeParser::default())),
        "dart" => ("Dart", Box::new(clike::ClikeParser::default())),
        "go" => ("Go", Box::new(clike::ClikeParser::default())),
        "java" => ("Java", Box::new(clike::ClikeParser::default())),
        "js" | "ts" | "jsx" | "tsx" => (
            "JavaScript/TypeScript",
            Box::new(clike::ClikeParser::default()),
        ),
        "kt" => ("Kotlin", Box::new(clike::ClikeParser::default())),
        "nim" => ("Nim", Box::new(clike::ClikeParser::default())),
        "odin" => ("Odin", Box::new(clike::ClikeParser::default())),
        "php" => ("PHP", Box::new(clike::ClikeParser::default())),
        "rs" => ("Rust", Box::new(clike::ClikeParser::default())),
        "scala" => ("Scala", Box::new(clike::ClikeParser::default())),
        "swift" => ("Swift", Box::new(clike::ClikeParser::default())),
        "v" => ("V", Box::new(clike::ClikeParser::default())),
        "zig" => ("Zig", Box::new(clike::ClikeParser::default())),

        // Lua style
        "elm" => ("Elm", Box::new(lua::Luaparser)),
        "haskell" | "hs" => ("Haskell", Box::new(lua::Luaparser)),
        "idris" => ("Idris", Box::new(lua::Luaparser)),
        "lua" => ("Lua", Box::new(lua::Luaparser)),
        "sql" => ("SQL", Box::new(lua::Luaparser)),

        // Shell style
        "bat" | "cmd" => ("Batch", Box::new(script::ScriptParser)),
        "cmake" => ("CMake", Box::new(script::ScriptParser)),
        "dockerfile" => ("Dockerfile", Box::new(script::ScriptParser)),
        "fish" => ("Fish Shell", Box::new(script::ScriptParser)),
        "gradle" => ("Gradle", Box::new(script::ScriptParser)),
        "groovy" => ("Groovy", Box::new(script::ScriptParser)),
        "ini" | "cfg" => ("INI", Box::new(script::ScriptParser)),
        "jl" => ("Julia", Box::new(script::ScriptParser)),
        "makefile" | "mk" => ("Makefile", Box::new(script::ScriptParser)),
        "nix" => ("Nix", Box::new(script::ScriptParser)),
        "pl" | "pm" => ("Perl", Box::new(script::ScriptParser)),
        "properties" => ("Properties", Box::new(script::ScriptParser)),
        "ps1" => ("PowerShell", Box::new(script::ScriptParser)),
        "py" | "pyw" => ("Python", Box::new(script::ScriptParser)),
        "r" => ("R", Box::new(script::ScriptParser)),
        "rb" => ("Ruby", Box::new(script::ScriptParser)),
        "sh" | "zsh" | "bash" => ("Shell", Box::new(script::ScriptParser)),
        "tcl" => ("Tcl", Box::new(script::ScriptParser)),
        "tf" | "hcl" => ("Terraform", Box::new(script::ScriptParser)),
        "toml" => ("TOML", Box::new(script::ScriptParser)),
        "vim" | "vimrc" => ("Vim Script", Box::new(script::ScriptParser)),
        "yaml" | "yml" => ("YAML", Box::new(script::ScriptParser)),

        _ => return None,
    };
    Some((name, parser))
}
