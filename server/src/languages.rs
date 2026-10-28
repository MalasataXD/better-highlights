use std::path::Path;

/// Per-language tree-sitter node kinds that drive complexity scoring and comment tagging.
/// Adding a language is a new entry in `for_path` plus its grammar crate.
pub struct Language {
    pub grammar: tree_sitter::Language,
    /// Named functions and methods. Each gets its own complexity hint.
    pub functions: &'static [&'static str],
    /// Anonymous functions. They add nesting to the enclosing function, and get their own hint
    /// only when they are not inside another function.
    pub lambdas: &'static [&'static str],
    /// `if` nodes, plus clauses that continue an `if` chain, such as Python's `elif_clause`.
    pub ifs: &'static [&'static str],
    /// Node wrapping an `if`'s alternative, when the grammar has one (`else_clause`).
    pub else_clause: Option<&'static str>,
    /// Loops, switches, catches and ternaries: +1 plus nesting, and nest their children.
    pub nesting: &'static [&'static str],
    /// Binary node whose `operator` field may be `&&`/`||` (or `and`/`or`).
    pub logical: &'static str,
    pub comments: &'static [&'static str],
    /// Comment openers, longest first, stripped before looking for a tag.
    pub comment_markers: &'static [&'static str],
}

impl Language {
    pub fn parse(&self, source: &str) -> Option<tree_sitter::Tree> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&self.grammar).ok()?;
        parser.parse(source, None)
    }
}

/// Picks the language for a document by file extension.
pub fn for_path(path: &Path) -> Option<Language> {
    let extension = path.extension()?.to_str()?;
    let language = match extension {
        "rs" => Language {
            grammar: tree_sitter_rust::LANGUAGE.into(),
            functions: &["function_item"],
            lambdas: &["closure_expression"],
            ifs: &["if_expression"],
            else_clause: Some("else_clause"),
            nesting: &[
                "for_expression",
                "while_expression",
                "loop_expression",
                "match_expression",
            ],
            logical: "binary_expression",
            comments: &["line_comment", "block_comment"],
            comment_markers: &["///", "//!", "/**", "/*!", "/*", "//"],
        },
        "ts" | "mts" | "cts" => js_like(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => js_like(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "js" | "jsx" | "mjs" | "cjs" => js_like(tree_sitter_javascript::LANGUAGE.into()),
        "cs" => Language {
            grammar: tree_sitter_c_sharp::LANGUAGE.into(),
            functions: &[
                "method_declaration",
                "constructor_declaration",
                "local_function_statement",
                "operator_declaration",
            ],
            lambdas: &["lambda_expression", "anonymous_method_expression"],
            ifs: &["if_statement"],
            else_clause: None,
            nesting: &[
                "for_statement",
                "foreach_statement",
                "while_statement",
                "do_statement",
                "switch_statement",
                "switch_expression",
                "catch_clause",
                "conditional_expression",
            ],
            logical: "binary_expression",
            comments: &["comment"],
            comment_markers: &["///", "/**", "/*", "//"],
        },
        "py" | "pyi" => Language {
            grammar: tree_sitter_python::LANGUAGE.into(),
            functions: &["function_definition"],
            lambdas: &["lambda"],
            ifs: &["if_statement", "elif_clause"],
            else_clause: Some("else_clause"),
            nesting: &[
                "for_statement",
                "while_statement",
                "match_statement",
                "except_clause",
                "conditional_expression",
            ],
            logical: "boolean_operator",
            comments: &["comment"],
            comment_markers: &["#"],
        },
        "go" => Language {
            grammar: tree_sitter_go::LANGUAGE.into(),
            functions: &["function_declaration", "method_declaration"],
            lambdas: &["func_literal"],
            ifs: &["if_statement"],
            else_clause: None,
            nesting: &[
                "for_statement",
                "expression_switch_statement",
                "type_switch_statement",
                "select_statement",
            ],
            logical: "binary_expression",
            comments: &["comment"],
            comment_markers: &["/*", "//"],
        },
        "java" => Language {
            grammar: tree_sitter_java::LANGUAGE.into(),
            functions: &[
                "method_declaration",
                "constructor_declaration",
                "compact_constructor_declaration",
            ],
            lambdas: &["lambda_expression"],
            ifs: &["if_statement"],
            else_clause: None,
            nesting: &[
                "for_statement",
                "enhanced_for_statement",
                "while_statement",
                "do_statement",
                "switch_expression",
                "catch_clause",
                "ternary_expression",
            ],
            logical: "binary_expression",
            comments: &["line_comment", "block_comment"],
            comment_markers: &["/**", "/*", "//"],
        },
        _ => return None,
    };
    Some(language)
}

fn js_like(grammar: tree_sitter::Language) -> Language {
    Language {
        grammar,
        functions: &[
            "function_declaration",
            "generator_function_declaration",
            "method_definition",
        ],
        lambdas: &[
            "arrow_function",
            "function_expression",
            "function",
            "generator_function",
        ],
        ifs: &["if_statement"],
        else_clause: Some("else_clause"),
        nesting: &[
            "for_statement",
            "for_in_statement",
            "while_statement",
            "do_statement",
            "switch_statement",
            "catch_clause",
            "ternary_expression",
        ],
        logical: "binary_expression",
        comments: &["comment"],
        comment_markers: &["/**", "/*", "//"],
    }
}
