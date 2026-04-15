//! LSP (Language Server Protocol) 服务器
//!
//! 为 IDE 提供代码补全、跳转定义、诊断等功能

use crate::ast::*;
use crate::error::{CompileError, Span};
use crate::lexer::token::{Token, TokenKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// LSP 服务器
pub struct LspServer {
    /// 文档内容
    documents: HashMap<String, String>,
    /// 已解析的 AST
    ast_cache: HashMap<String, Module>,
    /// 诊断信息
    diagnostics: HashMap<String, Vec<Diagnostic>>,
}

/// 诊断信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub source: String,
}

/// 诊断严重程度
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

/// 位置范围
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// 位置
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// 补全项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
}

/// 补全项类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(u8)]
pub enum CompletionItemKind {
    Text = 1,
    Method = 2,
    Function = 3,
    Constructor = 4,
    Field = 5,
    Variable = 6,
    Class = 7,
    Interface = 8,
    Module = 9,
    Property = 10,
    Unit = 11,
    Value = 12,
    Enum = 13,
    Keyword = 14,
    Snippet = 15,
    Color = 16,
    File = 17,
    Reference = 18,
    Folder = 19,
    EnumMember = 20,
    Constant = 21,
    Struct = 22,
    Event = 23,
    Operator = 24,
    TypeParameter = 25,
}

/// 符号信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInformation {
    pub name: String,
    pub kind: SymbolKind,
    pub location: Location,
    pub container_name: Option<String>,
}

/// 符号类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(u8)]
pub enum SymbolKind {
    File = 1,
    Module = 2,
    Namespace = 3,
    Package = 4,
    Class = 5,
    Method = 6,
    Property = 7,
    Field = 8,
    Constructor = 9,
    Enum = 10,
    Interface = 11,
    Function = 12,
    Variable = 13,
    Constant = 14,
    String = 15,
    Number = 16,
    Boolean = 17,
    Array = 18,
    Object = 19,
    Key = 20,
    Null = 21,
    EnumMember = 22,
    Struct = 23,
    Event = 24,
    Operator = 25,
    TypeParameter = 26,
}

/// 位置信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

/// 悬停信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hover {
    pub contents: String,
    pub range: Option<Range>,
}

impl LspServer {
    /// 创建新的 LSP 服务器
    pub fn new() -> Self {
        Self { documents: HashMap::new(), ast_cache: HashMap::new(), diagnostics: HashMap::new() }
    }

    /// 打开文档
    pub fn open_document(&mut self, uri: String, content: String) {
        self.documents.insert(uri.clone(), content.clone());
        self.parse_document(&uri, &content);
    }

    /// 更新文档
    pub fn update_document(&mut self, uri: String, content: String) {
        self.documents.insert(uri.clone(), content.clone());
        self.parse_document(&uri, &content);
    }

    /// 关闭文档
    pub fn close_document(&mut self, uri: &str) {
        self.documents.remove(uri);
        self.ast_cache.remove(uri);
        self.diagnostics.remove(uri);
    }

    /// 解析文档
    fn parse_document(&mut self, uri: &str, content: &str) {
        // 这里简化处理，实际应该调用完整的解析流程
        let diagnostics = Vec::new();
        self.diagnostics.insert(uri.to_string(), diagnostics);
    }

    /// 获取诊断信息
    pub fn get_diagnostics(&self, uri: &str) -> Vec<Diagnostic> {
        self.diagnostics.get(uri).cloned().unwrap_or_default()
    }

    /// 代码补全
    pub fn completion(&self, uri: &str, position: Position) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        // 关键字补全
        items.extend(self.keyword_completions());

        // 类型补全
        items.extend(self.type_completions());

        // 从 AST 获取符号
        if let Some(ast) = self.ast_cache.get(uri) {
            items.extend(self.symbol_completions(ast));
        }

        items
    }

    /// 关键字补全
    fn keyword_completions(&self) -> Vec<CompletionItem> {
        let keywords = vec![
            ("module", "module ${1:name};"),
            ("use", "use ${1:path};"),
            ("resource", "resource ${1:Name} {\n    $0\n}"),
            ("shared", "shared ${1:Name} {\n    $0\n}"),
            ("receipt", "receipt ${1:Name} {\n    $0\n}"),
            ("struct", "struct ${1:Name} {\n    $0\n}"),
            ("action", "action ${1:name}($2) {\n    $0\n}"),
            ("lock", "lock ${1:name}($2) -> $3 {\n    $0\n}"),
            ("let", "let ${1:name} = $0;"),
            ("if", "if ${1:condition} {\n    $0\n}"),
            ("for", "for ${1:item} in ${2:iterable} {\n    $0\n}"),
            ("while", "while ${1:condition} {\n    $0\n}"),
            ("return", "return $0;"),
            ("create", "create ${1:Type} { $0 }"),
            ("destroy", "destroy ${1:expr};"),
            ("transfer", "transfer ${1:expr} to ${2:addr};"),
            ("assert", "assert!(${1:condition});"),
        ];

        keywords
            .into_iter()
            .map(|(label, insert)| CompletionItem {
                label: label.to_string(),
                kind: CompletionItemKind::Keyword,
                detail: Some(format!("{} keyword", label)),
                documentation: None,
                insert_text: Some(insert.to_string()),
            })
            .collect()
    }

    /// 类型补全
    fn type_completions(&self) -> Vec<CompletionItem> {
        let types = vec![
            "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128", "bool", "String", "Address", "Hash", "Bytes", "Vec",
            "Option", "Result", "Map",
        ];

        types
            .into_iter()
            .map(|ty| CompletionItem {
                label: ty.to_string(),
                kind: CompletionItemKind::TypeParameter,
                detail: Some(format!("{} type", ty)),
                documentation: None,
                insert_text: None,
            })
            .collect()
    }

    /// 符号补全
    fn symbol_completions(&self, module: &Module) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        for item in &module.items {
            match item {
                Item::Resource(r) => {
                    items.push(CompletionItem {
                        label: r.name.clone(),
                        kind: CompletionItemKind::Struct,
                        detail: Some(format!("resource {}", r.name)),
                        documentation: None,
                        insert_text: Some(r.name.clone()),
                    });
                }
                Item::Shared(s) => {
                    items.push(CompletionItem {
                        label: s.name.clone(),
                        kind: CompletionItemKind::Struct,
                        detail: Some(format!("shared {}", s.name)),
                        documentation: None,
                        insert_text: Some(s.name.clone()),
                    });
                }
                Item::Receipt(r) => {
                    items.push(CompletionItem {
                        label: r.name.clone(),
                        kind: CompletionItemKind::Struct,
                        detail: Some(format!("receipt {}", r.name)),
                        documentation: None,
                        insert_text: Some(r.name.clone()),
                    });
                }
                Item::Struct(s) => {
                    items.push(CompletionItem {
                        label: s.name.clone(),
                        kind: CompletionItemKind::Struct,
                        detail: Some(format!("struct {}", s.name)),
                        documentation: None,
                        insert_text: Some(s.name.clone()),
                    });
                }
                Item::Action(a) => {
                    items.push(CompletionItem {
                        label: a.name.clone(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("action {}", a.name)),
                        documentation: a.doc_comment.clone(),
                        insert_text: Some(format!("{}($0)", a.name)),
                    });
                }
                Item::Lock(l) => {
                    items.push(CompletionItem {
                        label: l.name.clone(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("lock {}", l.name)),
                        documentation: None,
                        insert_text: Some(format!("{}($0)", l.name)),
                    });
                }
                _ => {}
            }
        }

        items
    }

    /// 跳转到定义
    pub fn goto_definition(&self, uri: &str, position: Position) -> Option<Location> {
        // 简化实现：查找符号位置
        None
    }

    /// 查找所有引用
    pub fn find_references(&self, uri: &str, position: Position) -> Vec<Location> {
        Vec::new()
    }

    /// 悬停提示
    pub fn hover(&self, uri: &str, position: Position) -> Option<Hover> {
        if let Some(ast) = self.ast_cache.get(uri) {
            // 查找位置对应的符号
            for item in &ast.items {
                if let Some(hover) = self.item_hover(item, position) {
                    return Some(hover);
                }
            }
        }
        None
    }

    /// 获取条目的悬停信息
    fn item_hover(&self, item: &Item, position: Position) -> Option<Hover> {
        match item {
            Item::Resource(r) => Some(Hover {
                contents: format!("```cellscript\nresource {}\n```\n\nCapabilities: {:?}", r.name, r.capabilities),
                range: None,
            }),
            Item::Action(a) => Some(Hover {
                contents: format!(
                    "```cellscript\naction {}\n```\n\n{}",
                    a.name,
                    a.doc_comment.as_deref().unwrap_or("No documentation")
                ),
                range: None,
            }),
            _ => None,
        }
    }

    /// 文档符号
    pub fn document_symbols(&self, uri: &str) -> Vec<SymbolInformation> {
        let mut symbols = Vec::new();

        if let Some(ast) = self.ast_cache.get(uri) {
            for item in &ast.items {
                if let Some(symbol) = self.item_symbol(item, uri) {
                    symbols.push(symbol);
                }
            }
        }

        symbols
    }

    /// 获取条目的符号信息
    fn item_symbol(&self, item: &Item, uri: &str) -> Option<SymbolInformation> {
        match item {
            Item::Resource(r) => Some(SymbolInformation {
                name: r.name.clone(),
                kind: SymbolKind::Struct,
                location: Location { uri: uri.to_string(), range: span_to_range(r.span) },
                container_name: None,
            }),
            Item::Action(a) => Some(SymbolInformation {
                name: a.name.clone(),
                kind: SymbolKind::Function,
                location: Location { uri: uri.to_string(), range: span_to_range(a.span) },
                container_name: None,
            }),
            _ => None,
        }
    }

    /// 重命名符号
    pub fn rename(&self, uri: &str, position: Position, new_name: String) -> HashMap<String, Vec<TextEdit>> {
        HashMap::new()
    }

    /// 代码操作
    pub fn code_action(&self, uri: &str, range: Range) -> Vec<CodeAction> {
        Vec::new()
    }

    /// 格式化文档
    pub fn format_document(&self, uri: &str) -> Vec<TextEdit> {
        Vec::new()
    }

    /// 格式化范围
    pub fn format_range(&self, uri: &str, range: Range) -> Vec<TextEdit> {
        Vec::new()
    }
}

/// 文本编辑
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

/// 代码操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeAction {
    pub title: String,
    pub kind: String,
    pub edit: Option<WorkspaceEdit>,
}

/// 工作区编辑
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEdit {
    pub changes: HashMap<String, Vec<TextEdit>>,
}

/// 将 Span 转换为 Range
fn span_to_range(span: Span) -> Range {
    Range {
        start: Position { line: span.start_line as u32, character: span.start_col as u32 },
        end: Position { line: span.end_line as u32, character: span.end_col as u32 },
    }
}

/// 将位置转换为 LSP 位置
fn pos_to_position(pos: usize, source: &str) -> Position {
    let mut line = 0;
    let mut col = 0;

    for (i, c) in source.char_indices() {
        if i >= pos {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    Position { line: line as u32, character: col as u32 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_server() {
        let mut server = LspServer::new();

        let uri = "file:///test.cell".to_string();
        let content = "module test;".to_string();

        server.open_document(uri.clone(), content);

        // 测试补全
        let completions = server.completion(&uri, Position { line: 0, character: 0 });
        assert!(!completions.is_empty());

        // 测试关键字补全
        let keywords: Vec<_> = completions.iter().filter(|c| c.kind == CompletionItemKind::Keyword).collect();
        assert!(!keywords.is_empty());
    }

    #[test]
    fn test_keyword_completions() {
        let server = LspServer::new();
        let keywords = server.keyword_completions();

        assert!(keywords.iter().any(|k| k.label == "module"));
        assert!(keywords.iter().any(|k| k.label == "resource"));
        assert!(keywords.iter().any(|k| k.label == "action"));
    }
}
