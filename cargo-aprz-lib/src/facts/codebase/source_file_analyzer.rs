use ra_ap_syntax::{
    AstNode, Edition, SourceFile, SyntaxKind, SyntaxNode,
    ast::{self, HasAttrs},
};
use std::collections::HashSet;

#[derive(Debug)]
pub struct SourceFileInfo {
    pub production_lines: u64,
    pub test_lines: u64,
    pub comment_lines: u64,
    pub unsafe_count: u64,
    pub has_errors: bool,
}

struct SourceFileAnalyzer<'a> {
    source: &'a str,
    line_starts: Vec<usize>,
    production_lines: HashSet<usize>,
    test_lines: HashSet<usize>,
    comment_lines: HashSet<usize>,
    unsafe_count: usize,
    test_context_depth: usize,
}

impl<'a> SourceFileAnalyzer<'a> {
    fn new(source: &'a str) -> Self {
        // Build line starts index for O(log n) line lookups
        let line_starts: Vec<usize> = core::iter::once(0)
            .chain(source.char_indices().filter_map(|(i, c)| (c == '\n').then_some(i + 1)))
            .collect();

        Self {
            source,
            line_starts,
            production_lines: HashSet::new(),
            test_lines: HashSet::new(),
            comment_lines: HashSet::new(),
            unsafe_count: 0,
            test_context_depth: 0,
        }
    }

    fn analyze(&mut self, node: &SyntaxNode) {
        // Check if this node is a test context marker
        let is_test_marker = Self::is_test_context(node);

        if is_test_marker {
            self.test_context_depth += 1;
        }

        // Process the current node
        self.process_node(node);

        // Process all child tokens and nodes
        for element in node.children_with_tokens() {
            match element {
                ra_ap_syntax::NodeOrToken::Node(child_node) => {
                    self.analyze(&child_node);
                }
                ra_ap_syntax::NodeOrToken::Token(token) => {
                    self.process_token(&token);
                }
            }
        }

        // Restore test context depth
        if is_test_marker {
            self.test_context_depth -= 1;
        }
    }

    fn process_node(&mut self, node: &SyntaxNode) {
        // Record code lines (non-comment, non-whitespace-only)
        if Self::is_code_node(node) {
            self.record_code_lines(node);
        }
    }

    fn process_token(&mut self, token: &ra_ap_syntax::SyntaxToken) {
        let kind = token.kind();

        // Handle comments
        if matches!(kind, SyntaxKind::COMMENT) {
            self.record_comment_lines_from_token(token);
        }

        // Count unsafe keywords
        if kind == SyntaxKind::UNSAFE_KW {
            // Check if this is part of an unsafe block, fn, impl, or trait
            if let Some(parent) = token.parent() {
                match parent.kind() {
                    SyntaxKind::BLOCK_EXPR | // unsafe { }
                    SyntaxKind::FN | // unsafe fn
                    SyntaxKind::IMPL | // unsafe impl
                    SyntaxKind::TRAIT => { // unsafe trait
                        self.unsafe_count += 1;
                    }
                    _ => {}
                }
            }
        }
    }

    /// Check if a node represents a test context (module or function with test attributes)
    fn is_test_context(node: &SyntaxNode) -> bool {
        match node.kind() {
            SyntaxKind::MODULE => {
                if let Some(module) = ast::Module::cast(node.clone()) {
                    return Self::has_test_attribute(&module);
                }
            }
            SyntaxKind::FN => {
                if let Some(function) = ast::Fn::cast(node.clone()) {
                    return Self::has_test_attribute(&function);
                }
            }
            _ => {}
        }
        false
    }

    /// Check if a node with attributes contains test-related attributes
    fn has_test_attribute<T: HasAttrs>(node: &T) -> bool {
        for attr in node.attrs() {
            // Get the path as a string
            let attr_text = attr.syntax().text().to_string();

            // Check for #[test]
            if attr_text.contains("test") && !attr_text.contains("cfg") {
                return true;
            }

            // Check for #[cfg(test)]
            if attr_text.contains("cfg") && attr_text.contains("test") {
                return true;
            }
        }
        false
    }

    /// Check if a node represents actual code (not just structural elements)
    fn is_code_node(node: &SyntaxNode) -> bool {
        matches!(
            node.kind(),
            SyntaxKind::EXPR_STMT
                | SyntaxKind::LET_STMT
                | SyntaxKind::ITEM_LIST
                | SyntaxKind::FN
                | SyntaxKind::STRUCT
                | SyntaxKind::ENUM
                | SyntaxKind::TRAIT
                | SyntaxKind::IMPL
                | SyntaxKind::CONST
                | SyntaxKind::STATIC
                | SyntaxKind::TYPE_ALIAS
                | SyntaxKind::USE
                | SyntaxKind::MACRO_CALL
                | SyntaxKind::MACRO_RULES
                | SyntaxKind::MACRO_DEF
        )
    }

    /// Record lines that contain comments from a token
    fn record_comment_lines_from_token(&mut self, token: &ra_ap_syntax::SyntaxToken) {
        let text_range = token.text_range();
        let start = text_range.start().into();
        let end = text_range.end().into();

        let start_line = self.offset_to_line(start);
        let end_line = self.offset_to_line(end);

        for line in start_line..=end_line {
            let _ = self.comment_lines.insert(line);
        }
    }

    /// Record lines that contain code
    fn record_code_lines(&mut self, node: &SyntaxNode) {
        let text_range = node.text_range();
        let start = text_range.start().into();
        let end = text_range.end().into();

        let start_line = self.offset_to_line(start);
        let end_line = self.offset_to_line(end);

        for line in start_line..=end_line {
            // Skip blank lines
            if self.is_blank_line(line) {
                continue;
            }

            // Add to appropriate set based on test context
            if self.test_context_depth > 0 {
                let _ = self.test_lines.insert(line);
            } else {
                let _ = self.production_lines.insert(line);
            }
        }
    }

    /// Check if a line is blank (whitespace only)
    fn is_blank_line(&self, line_number: usize) -> bool {
        self.get_line_text(line_number).is_none_or(|line_text| line_text.trim().is_empty())
    }

    /// Get the text of a specific line using the line starts index
    fn get_line_text(&self, line_number: usize) -> Option<&str> {
        let start = *self.line_starts.get(line_number)?;
        let end = self.line_starts.get(line_number + 1).copied().unwrap_or(self.source.len());
        self.source.get(start..end)
    }

    /// Convert byte offset to line number (0-indexed) using binary search
    fn offset_to_line(&self, offset: usize) -> usize {
        self.line_starts
            .binary_search(&offset)
            .unwrap_or_else(|line| line.saturating_sub(1))
    }
}

pub fn analyze_source_file(source_file: &str) -> SourceFileInfo {
    let parse = SourceFile::parse(source_file, Edition::CURRENT);
    let has_errors = !parse.errors().is_empty();
    let root = parse.tree().syntax().clone();

    let mut analyzer = SourceFileAnalyzer::new(source_file);
    analyzer.analyze(&root);

    SourceFileInfo {
        production_lines: analyzer.production_lines.len() as u64,
        test_lines: analyzer.test_lines.len() as u64,
        comment_lines: analyzer.comment_lines.len() as u64,
        unsafe_count: analyzer.unsafe_count as u64,
        has_errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "Miri detects UB in external rowan crate")]
    fn test_simple_production_code() {
        let source = r#"
fn main() {
    println!("Hello, world!");
}
"#;
        let stats = analyze_source_file(source);
        assert!(stats.production_lines > 0);
        assert_eq!(stats.test_lines, 0);
        assert_eq!(stats.unsafe_count, 0);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri detects UB in external rowan crate")]
    fn test_with_comments() {
        let source = r#"
// This is a comment
fn main() {
    // Another comment
    println!("Hello");
}
"#;
        let stats = analyze_source_file(source);
        assert!(stats.comment_lines >= 2);
        assert!(stats.production_lines > 0);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri detects UB in external rowan crate")]
    fn test_with_test_code() {
        let source = r#"
fn production_fn() {
    println!("Production");
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_something() {
        assert!(true);
    }
}
"#;
        let stats = analyze_source_file(source);
        assert!(stats.production_lines > 0);
        assert!(stats.test_lines > 0);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri detects UB in external rowan crate")]
    fn test_unsafe_counting() {
        let source = r#"
unsafe fn unsafe_function() {
    println!("Unsafe");
}

fn safe_function() {
    unsafe {
        // Unsafe block
    }
}

unsafe impl Send for MyType {}

unsafe trait UnsafeTrait {}
"#;
        let stats = analyze_source_file(source);
        // Should count: unsafe fn, unsafe block, unsafe impl, unsafe trait = 4
        assert!(stats.unsafe_count >= 4);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri detects UB in external rowan crate")]
    fn test_test_function_attribute() {
        let source = "
#[test]
fn this_is_a_test() {
    assert_eq!(2 + 2, 4);
}
";
        let stats = analyze_source_file(source);
        assert!(stats.test_lines > 0);
        assert_eq!(stats.production_lines, 0);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri detects UB in external rowan crate")]
    fn test_complex_example() {
        let source = "
//! Module documentation

/// A production struct
pub struct MyStruct {
    value: i32,
}

impl MyStruct {
    /// Creates a new instance
    pub fn new(value: i32) -> Self {
        Self { value }
    }

    /// Gets the value
    pub fn value(&self) -> i32 {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let s = MyStruct::new(42);
        assert_eq!(s.value(), 42);
    }

    #[test]
    fn test_value() {
        let s = MyStruct::new(100);
        assert_eq!(s.value(), 100);
    }
}
";
        let stats = analyze_source_file(source);
        assert!(stats.production_lines > 0);
        assert!(stats.test_lines > 0);
        assert!(stats.comment_lines > 0);
        assert_eq!(stats.unsafe_count, 0);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri detects UB in external rowan crate")]
    fn test_empty_source() {
        let stats = analyze_source_file("");
        assert_eq!(stats.production_lines, 0);
        assert_eq!(stats.test_lines, 0);
        assert_eq!(stats.comment_lines, 0);
        assert_eq!(stats.unsafe_count, 0);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri detects UB in external rowan crate")]
    fn test_only_whitespace() {
        let source = "   \n\n   \n  ";
        let stats = analyze_source_file(source);
        assert_eq!(stats.production_lines, 0);
        assert_eq!(stats.test_lines, 0);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri detects UB in external rowan crate")]
    fn test_invalid_rust_code() {
        // Test that invalid Rust code doesn't panic or return errors
        // The rowan parser is error-tolerant and always produces a tree

        // Complete garbage
        let stats1 = analyze_source_file("this is not rust code at all !@#$%");
        // Should not panic, though stats might be zero or inaccurate
        assert_eq!(stats1.unsafe_count, 0);
        assert!(stats1.has_errors, "Garbage code should have parse errors");

        // Incomplete syntax
        let stats2 = analyze_source_file("fn incomplete(");
        // Should not panic
        assert_eq!(stats2.unsafe_count, 0);
        assert!(stats2.has_errors, "Incomplete syntax should have parse errors");

        // Syntax errors
        let stats3 = analyze_source_file("fn bad() { let x = ; }");
        // Should not panic
        assert_eq!(stats3.unsafe_count, 0);
        assert!(stats3.has_errors, "Syntax errors should be detected");

        // Mixed valid and invalid
        let stats4 = analyze_source_file("fn valid() {} !@#$ nonsense fn another() {}");
        // Should not panic - might parse what it can
        assert_eq!(stats4.unsafe_count, 0);
        assert!(stats4.has_errors, "Mixed valid/invalid should have parse errors");

        // Valid code should have no errors
        let stats5 = analyze_source_file("fn valid() { println!(\"test\"); }");
        assert!(!stats5.has_errors, "Valid code should not have parse errors");
    }
}
