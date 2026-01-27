//! AST-based code parsing via tree-sitter

use anyhow::{Context, Result};
use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor};

/// Types of symbols we index
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolType {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Module,
    Const,
    Static,
    TypeAlias,
    Macro,
}

impl SymbolType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Impl => "impl",
            Self::Module => "module",
            Self::Const => "const",
            Self::Static => "static",
            Self::TypeAlias => "type_alias",
            Self::Macro => "macro",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "function" => Some(Self::Function),
            "struct" => Some(Self::Struct),
            "enum" => Some(Self::Enum),
            "trait" => Some(Self::Trait),
            "impl" => Some(Self::Impl),
            "module" => Some(Self::Module),
            "const" => Some(Self::Const),
            "static" => Some(Self::Static),
            "type_alias" => Some(Self::TypeAlias),
            "macro" => Some(Self::Macro),
            _ => None,
        }
    }
}

/// A parsed symbol from the source code
#[derive(Debug, Clone)]
pub struct ParsedSymbol {
    pub symbol_type: SymbolType,
    pub name: String,
    pub full_path: String,
    pub line_start: usize,
    pub line_end: usize,
    pub signature: Option<String>,
    pub calls: Vec<String>,
}

/// Rust source code parser using tree-sitter
pub struct RustParser {
    parser: Parser,
    language: Language,
}

impl RustParser {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        let language: Language = tree_sitter_rust::LANGUAGE.into();
        parser
            .set_language(&language)
            .context("Failed to set Rust language")?;
        Ok(Self { parser, language })
    }

    /// Parse a Rust source file and extract symbols
    pub fn parse_file(&mut self, path: &Path, source: &str) -> Result<Vec<ParsedSymbol>> {
        let tree = self
            .parser
            .parse(source, None)
            .context("Failed to parse source")?;

        let module_path = self.path_to_module(path);
        let mut symbols = Vec::new();

        self.extract_symbols(tree.root_node(), source, &module_path, &mut symbols)?;

        Ok(symbols)
    }

    /// Convert file path to Rust module path (best effort)
    fn path_to_module(&self, path: &Path) -> String {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

        // Build path from components, skipping src/ and removing mod.rs/lib.rs
        let components: Vec<_> = path
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .skip_while(|&s| s != "src" && s != "lib")
            .skip(1) // Skip "src" or "lib"
            .filter(|&s| s != "mod.rs" && s != "lib.rs")
            .map(|s| s.strip_suffix(".rs").unwrap_or(s))
            .collect();

        if components.is_empty() {
            if stem == "lib" || stem == "main" {
                "crate".to_string()
            } else {
                stem.to_string()
            }
        } else {
            components.join("::")
        }
    }

    /// Extract symbols from AST using tree-sitter queries
    fn extract_symbols(
        &self,
        root: Node,
        source: &str,
        module_path: &str,
        symbols: &mut Vec<ParsedSymbol>,
    ) -> Result<()> {
        // Query for top-level items
        let query_str = r#"
            (function_item name: (identifier) @fn_name) @function
            (struct_item name: (type_identifier) @struct_name) @struct
            (enum_item name: (type_identifier) @enum_name) @enum
            (trait_item name: (type_identifier) @trait_name) @trait
            (impl_item type: (type_identifier) @impl_name) @impl
            (mod_item name: (identifier) @mod_name) @module
            (const_item name: (identifier) @const_name) @const
            (static_item name: (identifier) @static_name) @static
            (type_item name: (type_identifier) @type_name) @type_alias
            (macro_definition name: (identifier) @macro_name) @macro
        "#;

        let query = Query::new(&self.language, query_str).context("Failed to compile query")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source.as_bytes());

        while let Some(match_) = matches.next() {
            for capture in match_.captures.iter() {
                let node = capture.node;
                let capture_name = query.capture_names()[capture.index as usize];

                // Skip non-name captures
                if !capture_name.ends_with("_name") {
                    continue;
                }

                let name = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                if name.is_empty() {
                    continue;
                }

                let parent = node.parent();
                let (symbol_type, signature) = match capture_name {
                    "fn_name" => (
                        SymbolType::Function,
                        parent.map(|p| self.extract_function_signature(p, source)),
                    ),
                    "struct_name" => (SymbolType::Struct, None),
                    "enum_name" => (SymbolType::Enum, None),
                    "trait_name" => (SymbolType::Trait, None),
                    "impl_name" => (SymbolType::Impl, None),
                    "mod_name" => (SymbolType::Module, None),
                    "const_name" => (SymbolType::Const, None),
                    "static_name" => (SymbolType::Static, None),
                    "type_name" => (SymbolType::TypeAlias, None),
                    "macro_name" => (SymbolType::Macro, None),
                    _ => continue,
                };

                let full_path = if module_path.is_empty() {
                    name.clone()
                } else {
                    format!("{}::{}", module_path, name)
                };

                let parent_node = parent.unwrap_or(node);
                let calls = self.extract_function_calls(parent_node, source);

                symbols.push(ParsedSymbol {
                    symbol_type,
                    name,
                    full_path,
                    line_start: parent_node.start_position().row + 1,
                    line_end: parent_node.end_position().row + 1,
                    signature,
                    calls,
                });
            }
        }

        Ok(())
    }

    /// Extract function signature (parameters and return type)
    fn extract_function_signature(&self, node: Node, source: &str) -> String {
        // Get the first line of the function up to the opening brace
        let text = node.utf8_text(source.as_bytes()).unwrap_or("");
        text.lines()
            .next()
            .unwrap_or("")
            .trim_end_matches('{')
            .trim()
            .to_string()
    }

    /// Extract function calls within a node (simple heuristic)
    fn extract_function_calls(&self, node: Node, source: &str) -> Vec<String> {
        let mut calls = Vec::new();
        let call_query = r#"(call_expression function: (identifier) @call)"#;

        if let Ok(query) = Query::new(&self.language, call_query) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, node, source.as_bytes());
            while let Some(match_) = matches.next() {
                for capture in match_.captures.iter() {
                    if let Ok(text) = capture.node.utf8_text(source.as_bytes()) {
                        if !calls.contains(&text.to_string()) {
                            calls.push(text.to_string());
                        }
                    }
                }
            }
        }

        calls
    }
}

impl Default for RustParser {
    fn default() -> Self {
        Self::new().expect("Failed to create RustParser")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_function() {
        let mut parser = RustParser::new().unwrap();
        let source = r#"
            pub fn hello_world(name: &str) -> String {
                format!("Hello, {}!", name)
            }
        "#;
        let path = Path::new("src/lib.rs");
        let symbols = parser.parse_file(path, source).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].symbol_type, SymbolType::Function);
        assert_eq!(symbols[0].name, "hello_world");
    }

    #[test]
    fn test_parse_struct() {
        let mut parser = RustParser::new().unwrap();
        let source = r#"
            pub struct User {
                name: String,
                age: u32,
            }
        "#;
        let path = Path::new("src/models/user.rs");
        let symbols = parser.parse_file(path, source).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].symbol_type, SymbolType::Struct);
        assert_eq!(symbols[0].name, "User");
        assert!(symbols[0].full_path.contains("user"));
    }
}
