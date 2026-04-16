//! LSP (Language Server Protocol) 服务器
//!
//! 为 IDE 提供代码补全、跳转定义、诊断等功能

use crate::ast::*;
use crate::error::{CompileError, Span};
use camino::Utf8PathBuf;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
        self.ast_cache.remove(uri);

        let tokens = match crate::lexer::lex(content) {
            Ok(tokens) => tokens,
            Err(error) => {
                self.diagnostics.insert(uri.to_string(), vec![diagnostic_from_error(&error)]);
                return;
            }
        };

        let ast = match crate::parser::parse(&tokens) {
            Ok(ast) => ast,
            Err(error) => {
                self.diagnostics.insert(uri.to_string(), vec![diagnostic_from_error(&error)]);
                return;
            }
        };

        self.ast_cache.insert(uri.to_string(), ast.clone());
        let diagnostics = match crate::types::check(&ast).and_then(|_| crate::lifecycle::check(&ast)) {
            Ok(()) => {
                let mut diagnostics = Vec::new();
                if let Ok(metadata) = crate::compile_metadata(content, None) {
                    diagnostics.extend(lowering_diagnostics(&ast, &metadata));
                }
                diagnostics
            }
            Err(error) => vec![diagnostic_from_error(&error)],
        };
        self.diagnostics.insert(uri.to_string(), diagnostics);
    }

    /// 获取诊断信息
    pub fn get_diagnostics(&self, uri: &str) -> Vec<Diagnostic> {
        self.diagnostics.get(uri).cloned().unwrap_or_default()
    }

    /// 代码补全
    pub fn completion(&self, uri: &str, _position: Position) -> Vec<CompletionItem> {
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
        let symbol = self.symbol_at_position(uri, position)?;
        self.find_top_level_symbol(uri, &symbol)
    }

    /// 查找所有引用
    pub fn find_references(&self, uri: &str, position: Position) -> Vec<Location> {
        let Some(symbol) = self.symbol_at_position(uri, position) else {
            return Vec::new();
        };
        let mut refs = Vec::new();

        let workspace_modules = self.workspace_modules(uri);
        if !workspace_modules.is_empty() {
            for module in workspace_modules {
                let module_uri = utf8_path_to_file_uri(&module.path);
                for (start, end) in word_occurrences(&module.source, &symbol) {
                    refs.push(Location {
                        uri: module_uri.clone(),
                        range: Range {
                            start: offset_to_position(&module.source, start),
                            end: offset_to_position(&module.source, end),
                        },
                    });
                }
            }
            return refs;
        }

        if let Some(content) = self.documents.get(uri) {
            for (start, end) in word_occurrences(content, &symbol) {
                refs.push(Location {
                    uri: uri.to_string(),
                    range: Range { start: offset_to_position(content, start), end: offset_to_position(content, end) },
                });
            }
        }
        refs
    }

    /// 悬停提示
    pub fn hover(&self, uri: &str, position: Position) -> Option<Hover> {
        let symbol = self.symbol_at_position(uri, position)?;
        if let Some(ast) = self.ast_cache.get(uri) {
            let metadata = self.documents.get(uri).and_then(|source| crate::compile_metadata(source, None).ok());
            if let Some(hover) = ast.items.iter().find_map(|item| {
                if item_name(item) == Some(symbol.as_str()) {
                    self.item_hover(item, metadata.as_ref())
                } else {
                    None
                }
            }) {
                return Some(hover);
            }
        }

        for module in self.workspace_modules(uri) {
            let metadata = crate::compile_metadata(&module.source, None).ok();
            if let Some(hover) = module.ast.items.iter().find_map(|item| {
                if item_name(item) == Some(symbol.as_str()) {
                    self.item_hover(item, metadata.as_ref())
                } else {
                    None
                }
            }) {
                return Some(hover);
            }
        }

        None
    }

    /// 获取条目的悬停信息
    fn item_hover(&self, item: &Item, metadata: Option<&crate::CompileMetadata>) -> Option<Hover> {
        let range = span_to_range(item_span(item));
        match item {
            Item::Resource(r) => Some(Hover {
                contents: format!("```cellscript\nresource {}\n```\n\nCapabilities: {:?}", r.name, r.capabilities),
                range: Some(range),
            }),
            Item::Shared(s) => Some(Hover { contents: format!("```cellscript\nshared {}\n```", s.name), range: Some(range) }),
            Item::Receipt(r) => Some(Hover {
                contents: format!("```cellscript\nreceipt {}\n```{}", r.name, receipt_lifecycle_hover(r, metadata)),
                range: Some(range),
            }),
            Item::Struct(s) => Some(Hover { contents: format!("```cellscript\nstruct {}\n```", s.name), range: Some(range) }),
            Item::Action(a) => Some(Hover {
                contents: format!(
                    "```cellscript\naction {}\n```\n\n{}{}",
                    a.name,
                    a.doc_comment.as_deref().unwrap_or("No documentation"),
                    action_metadata_hover(&a.name, metadata)
                ),
                range: Some(range),
            }),
            Item::Function(f) => Some(Hover {
                contents: format!("```cellscript\nfn {}\n```\n\n{}", f.name, f.doc_comment.as_deref().unwrap_or("No documentation")),
                range: Some(range),
            }),
            Item::Lock(l) => Some(Hover { contents: format!("```cellscript\nlock {}\n```", l.name), range: Some(range) }),
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
            Item::Shared(s) => Some(SymbolInformation {
                name: s.name.clone(),
                kind: SymbolKind::Struct,
                location: Location { uri: uri.to_string(), range: span_to_range(s.span) },
                container_name: None,
            }),
            Item::Receipt(r) => Some(SymbolInformation {
                name: r.name.clone(),
                kind: SymbolKind::Struct,
                location: Location { uri: uri.to_string(), range: span_to_range(r.span) },
                container_name: None,
            }),
            Item::Struct(s) => Some(SymbolInformation {
                name: s.name.clone(),
                kind: SymbolKind::Struct,
                location: Location { uri: uri.to_string(), range: span_to_range(s.span) },
                container_name: None,
            }),
            Item::Const(c) => Some(SymbolInformation {
                name: c.name.clone(),
                kind: SymbolKind::Constant,
                location: Location { uri: uri.to_string(), range: span_to_range(c.span) },
                container_name: None,
            }),
            Item::Enum(e) => Some(SymbolInformation {
                name: e.name.clone(),
                kind: SymbolKind::Enum,
                location: Location { uri: uri.to_string(), range: span_to_range(e.span) },
                container_name: None,
            }),
            Item::Action(a) => Some(SymbolInformation {
                name: a.name.clone(),
                kind: SymbolKind::Function,
                location: Location { uri: uri.to_string(), range: span_to_range(a.span) },
                container_name: None,
            }),
            Item::Function(f) => Some(SymbolInformation {
                name: f.name.clone(),
                kind: SymbolKind::Function,
                location: Location { uri: uri.to_string(), range: span_to_range(f.span) },
                container_name: None,
            }),
            Item::Lock(l) => Some(SymbolInformation {
                name: l.name.clone(),
                kind: SymbolKind::Function,
                location: Location { uri: uri.to_string(), range: span_to_range(l.span) },
                container_name: None,
            }),
            _ => None,
        }
    }

    /// 重命名符号
    pub fn rename(&self, uri: &str, position: Position, new_name: String) -> HashMap<String, Vec<TextEdit>> {
        let mut changes = HashMap::new();
        let refs = self.find_references(uri, position);
        if refs.is_empty() {
            return changes;
        }
        let edits = refs.into_iter().map(|location| TextEdit { range: location.range, new_text: new_name.clone() }).collect();
        changes.insert(uri.to_string(), edits);
        changes
    }

    /// 代码操作
    pub fn code_action(&self, uri: &str, range: Range) -> Vec<CodeAction> {
        let mut actions = Vec::new();
        let has_lowering_diagnostic = self
            .diagnostics
            .get(uri)
            .into_iter()
            .flatten()
            .any(|diagnostic| diagnostic.source == "cellscript-lowering" && ranges_overlap(diagnostic.range, range));

        if has_lowering_diagnostic {
            actions.push(CodeAction {
                title: "Inspect lowering/runtime metadata with `cellc metadata`".to_string(),
                kind: "quickfix".to_string(),
                edit: None,
            });
            actions.push(CodeAction {
                title: "Use `--target riscv64-asm` until executable stateful lowering is implemented".to_string(),
                kind: "quickfix".to_string(),
                edit: None,
            });
        }

        actions
    }

    /// 格式化文档
    pub fn format_document(&self, uri: &str) -> Vec<TextEdit> {
        let Some(content) = self.documents.get(uri) else {
            return Vec::new();
        };
        let Some(ast) = self.ast_cache.get(uri) else {
            return Vec::new();
        };
        let Ok(formatted) = crate::fmt::format_default(ast) else {
            return Vec::new();
        };
        if &formatted == content {
            return Vec::new();
        }
        vec![TextEdit { range: Range { start: Position { line: 0, character: 0 }, end: end_position(content) }, new_text: formatted }]
    }

    /// 格式化范围
    pub fn format_range(&self, uri: &str, _range: Range) -> Vec<TextEdit> {
        self.format_document(uri)
    }

    fn symbol_at_position(&self, uri: &str, position: Position) -> Option<String> {
        let content = self.documents.get(uri)?;
        let offset = position_to_offset(content, position)?;
        word_at_offset(content, offset)
    }

    fn find_top_level_symbol(&self, uri: &str, symbol: &str) -> Option<Location> {
        if let Some(ast) = self.ast_cache.get(uri) {
            if let Some(location) = ast.items.iter().find_map(|item| {
                let name = item_name(item)?;
                if name == symbol {
                    Some(Location { uri: uri.to_string(), range: span_to_range(item_span(item)) })
                } else {
                    None
                }
            }) {
                return Some(location);
            }
        }

        for module in self.workspace_modules(uri) {
            if let Some(location) = module.ast.items.iter().find_map(|item| {
                let name = item_name(item)?;
                if name == symbol {
                    Some(Location { uri: utf8_path_to_file_uri(&module.path), range: span_to_range(item_span(item)) })
                } else {
                    None
                }
            }) {
                return Some(location);
            }
        }

        None
    }

    fn workspace_modules(&self, uri: &str) -> Vec<crate::LoadedModule> {
        let Some(path) = file_uri_to_utf8_path(uri) else {
            return Vec::new();
        };

        let mut modules = crate::load_modules_for_input(&path).unwrap_or_default();

        if let (Some(content), Some(ast)) = (self.documents.get(uri), self.ast_cache.get(uri)) {
            if let Some(module) = modules.iter_mut().find(|module| same_workspace_path(&module.path, &path)) {
                module.source = content.clone();
                module.ast = ast.clone();
            } else {
                modules.push(crate::LoadedModule { path, source: content.clone(), ast: ast.clone() });
            }
        }

        modules
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
        start: Position { line: span.line.saturating_sub(1) as u32, character: span.column.saturating_sub(1) as u32 },
        end: Position { line: span.line.saturating_sub(1) as u32, character: span.column.saturating_sub(1) as u32 },
    }
}

fn diagnostic_from_error(error: &CompileError) -> Diagnostic {
    Diagnostic {
        range: span_to_range(error.span),
        severity: DiagnosticSeverity::Error,
        message: error.message.clone(),
        source: "cellscript".to_string(),
    }
}

fn lowering_diagnostics(module: &Module, metadata: &crate::CompileMetadata) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for action in &metadata.actions {
        if action.elf_compatible {
            continue;
        }
        let span = module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Action(def) if def.name == action.name => Some(def.span),
                _ => None,
            })
            .unwrap_or_default();
        diagnostics.push(Diagnostic {
            range: span_to_range(span),
            severity: DiagnosticSeverity::Warning,
            message: format!(
                "action '{}' is not currently ELF-compatible; symbolic runtime features: {}; fail-closed runtime features: {}; CKB runtime features: {}; CKB accesses: {}",
                action.name,
                diagnostic_list(&action.symbolic_runtime_features),
                diagnostic_list(&action.fail_closed_runtime_features),
                diagnostic_list(&action.ckb_runtime_features),
                diagnostic_access_list(&action.ckb_runtime_accesses)
            ),
            source: "cellscript-lowering".to_string(),
        });
    }

    for lock in &metadata.locks {
        if lock.elf_compatible {
            continue;
        }
        let span = module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Lock(def) if def.name == lock.name => Some(def.span),
                _ => None,
            })
            .unwrap_or_default();
        diagnostics.push(Diagnostic {
            range: span_to_range(span),
            severity: DiagnosticSeverity::Warning,
            message: format!(
                "lock '{}' is not currently ELF-compatible; symbolic runtime features: {}; fail-closed runtime features: {}; CKB runtime features: {}; CKB accesses: {}",
                lock.name,
                diagnostic_list(&lock.symbolic_runtime_features),
                diagnostic_list(&lock.fail_closed_runtime_features),
                diagnostic_list(&lock.ckb_runtime_features),
                diagnostic_access_list(&lock.ckb_runtime_accesses)
            ),
            source: "cellscript-lowering".to_string(),
        });
    }

    diagnostics
}

fn diagnostic_list(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}

fn diagnostic_access_list(accesses: &[crate::CkbRuntimeAccessMetadata]) -> String {
    if accesses.is_empty() {
        return "none".to_string();
    }
    accesses
        .iter()
        .map(|access| format!("{}:{}#{} ({})", access.operation, access.source, access.index, access.binding))
        .collect::<Vec<_>>()
        .join(", ")
}

fn item_name(item: &Item) -> Option<&str> {
    match item {
        Item::Resource(r) => Some(&r.name),
        Item::Shared(s) => Some(&s.name),
        Item::Receipt(r) => Some(&r.name),
        Item::Struct(s) => Some(&s.name),
        Item::Const(c) => Some(&c.name),
        Item::Enum(e) => Some(&e.name),
        Item::Action(a) => Some(&a.name),
        Item::Function(f) => Some(&f.name),
        Item::Lock(l) => Some(&l.name),
        Item::Use(_) => None,
    }
}

fn item_span(item: &Item) -> Span {
    match item {
        Item::Resource(r) => r.span,
        Item::Shared(s) => s.span,
        Item::Receipt(r) => r.span,
        Item::Struct(s) => s.span,
        Item::Const(c) => c.span,
        Item::Enum(e) => e.span,
        Item::Action(a) => a.span,
        Item::Function(f) => f.span,
        Item::Lock(l) => l.span,
        Item::Use(u) => u.span,
    }
}

fn receipt_lifecycle_hover(receipt: &ReceiptDef, metadata: Option<&crate::CompileMetadata>) -> String {
    if let Some(type_metadata) =
        metadata.and_then(|metadata| metadata.types.iter().find(|type_metadata| type_metadata.name == receipt.name))
    {
        if type_metadata.lifecycle_states.is_empty() {
            return String::new();
        }

        let transitions = if type_metadata.lifecycle_transitions.is_empty() {
            "none".to_string()
        } else {
            type_metadata
                .lifecycle_transitions
                .iter()
                .map(|transition| {
                    format!("{}[{}] -> {}[{}]", transition.from, transition.from_index, transition.to, transition.to_index)
                })
                .collect::<Vec<_>>()
                .join(", ")
        };

        return format!(
            "\n\n**Lifecycle metadata**\n\nStates: `{}`\n\nTransitions: `{}`",
            type_metadata.lifecycle_states.join(" -> "),
            transitions
        );
    }

    let Some(lifecycle) = &receipt.lifecycle else {
        return String::new();
    };
    let transitions = lifecycle.states.windows(2).map(|window| format!("{} -> {}", window[0], window[1])).collect::<Vec<_>>();
    let transitions = if transitions.is_empty() { "none".to_string() } else { transitions.join(", ") };
    format!("\n\n**Lifecycle**\n\nStates: `{}`\n\nTransitions: `{}`", lifecycle.states.join(" -> "), transitions)
}

fn action_metadata_hover(name: &str, metadata: Option<&crate::CompileMetadata>) -> String {
    let Some(metadata) = metadata else {
        return String::new();
    };
    let Some(action) = metadata.actions.iter().find(|action| action.name == name) else {
        return String::new();
    };

    let features =
        if action.symbolic_runtime_features.is_empty() { "none".to_string() } else { action.symbolic_runtime_features.join(", ") };
    let fail_closed_features = if action.fail_closed_runtime_features.is_empty() {
        "none".to_string()
    } else {
        action.fail_closed_runtime_features.join(", ")
    };
    let ckb_features =
        if action.ckb_runtime_features.is_empty() { "none".to_string() } else { action.ckb_runtime_features.join(", ") };
    let accesses = if action.ckb_runtime_accesses.is_empty() {
        "none".to_string()
    } else {
        action
            .ckb_runtime_accesses
            .iter()
            .map(|access| format!("{}:{}#{} ({})", access.operation, access.source, access.index, access.binding))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let obligations = if action.verifier_obligations.is_empty() {
        "none".to_string()
    } else {
        action
            .verifier_obligations
            .iter()
            .map(|obligation| format!("{}:{} ({})", obligation.category, obligation.feature, obligation.status))
            .collect::<Vec<_>>()
            .join(", ")
    };

    format!(
        "\n\n**Lowering metadata**\n\nEffect: `{}`\n\nELF compatible: `{}`\n\nStandalone runner compatible: `{}`\n\nSymbolic runtime features: `{}`\n\nFail-closed runtime features: `{}`\n\nCKB runtime features: `{}`\n\nCKB runtime accesses: `{}`\n\nVerifier obligations: `{}`",
        action.effect_class,
        action.elf_compatible,
        action.standalone_runner_compatible,
        features,
        fail_closed_features,
        ckb_features,
        accesses,
        obligations
    )
}

fn position_to_offset(source: &str, position: Position) -> Option<usize> {
    let mut line = 0u32;
    let mut col = 0u32;

    for (idx, ch) in source.char_indices() {
        if line == position.line && col == position.character {
            return Some(idx);
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    if line == position.line && col == position.character {
        Some(source.len())
    } else {
        None
    }
}

fn offset_to_position(source: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;
    for (idx, ch) in source.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Position { line, character: col }
}

fn end_position(source: &str) -> Position {
    offset_to_position(source, source.len())
}

fn ranges_overlap(left: Range, right: Range) -> bool {
    position_le(left.start, right.end) && position_le(right.start, left.end)
}

fn position_le(left: Position, right: Position) -> bool {
    left.line < right.line || (left.line == right.line && left.character <= right.character)
}

fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn word_at_offset(source: &str, offset: usize) -> Option<String> {
    if source.is_empty() || offset > source.len() {
        return None;
    }
    let mut start = offset;
    while start > 0 {
        let prev_idx = source[..start].char_indices().last()?.0;
        let ch = source[prev_idx..start].chars().next()?;
        if !is_ident_char(ch) {
            break;
        }
        start = prev_idx;
    }

    let mut end = offset;
    while end < source.len() {
        let ch = source[end..].chars().next()?;
        if !is_ident_char(ch) {
            break;
        }
        end += ch.len_utf8();
    }

    if start == end {
        None
    } else {
        Some(source[start..end].to_string())
    }
}

fn word_occurrences(source: &str, symbol: &str) -> Vec<(usize, usize)> {
    let mut matches = Vec::new();
    let bytes = source.as_bytes();
    let needle = symbol.as_bytes();
    if needle.is_empty() {
        return matches;
    }

    let mut idx = 0;
    while idx + needle.len() <= bytes.len() {
        if &bytes[idx..idx + needle.len()] == needle {
            let before_ok = idx == 0 || !is_ident_char(source[..idx].chars().last().unwrap_or(' '));
            let after_ok =
                idx + needle.len() == bytes.len() || !is_ident_char(source[idx + needle.len()..].chars().next().unwrap_or(' '));
            if before_ok && after_ok {
                matches.push((idx, idx + needle.len()));
            }
            idx += needle.len();
        } else {
            idx += 1;
        }
    }
    matches
}

fn file_uri_to_utf8_path(uri: &str) -> Option<Utf8PathBuf> {
    let path = uri.strip_prefix("file://")?;
    let decoded = percent_decode(path)?;
    let candidate = Utf8PathBuf::from(decoded);
    std::fs::canonicalize(&candidate).ok().and_then(|path| Utf8PathBuf::from_path_buf(path).ok()).or(Some(candidate))
}

fn utf8_path_to_file_uri(path: &camino::Utf8Path) -> String {
    format!("file://{}", path)
}

fn same_workspace_path(left: &camino::Utf8Path, right: &camino::Utf8Path) -> bool {
    left == right
        || std::fs::canonicalize(left).ok().zip(std::fs::canonicalize(right).ok()).map(|(left, right)| left == right).unwrap_or(false)
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'%' {
            if idx + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_nibble(bytes[idx + 1])?;
            let lo = hex_nibble(bytes[idx + 2])?;
            out.push((hi << 4) | lo);
            idx += 3;
        } else {
            out.push(bytes[idx]);
            idx += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(10 + byte - b'a'),
        b'A'..=b'F' => Some(10 + byte - b'A'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_lsp_server() {
        let mut server = LspServer::new();

        let uri = "file:///test.cell".to_string();
        let content = "module test;\n\naction answer() -> u64 {\n    42\n}\n".to_string();

        server.open_document(uri.clone(), content);
        assert!(server.get_diagnostics(&uri).is_empty());

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

    #[test]
    fn test_parse_errors_become_diagnostics() {
        let mut server = LspServer::new();
        let uri = "file:///bad.cell".to_string();
        server.open_document(uri.clone(), "module bad;\naction broken( {\n".to_string());
        let diagnostics = server.get_diagnostics(&uri);
        assert!(!diagnostics.is_empty());
        assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Error);
    }

    #[test]
    fn test_goto_definition_and_references() {
        let mut server = LspServer::new();
        let uri = "file:///defs.cell".to_string();
        let source = "module defs;\n\nresource Token {\n    amount: u64,\n}\n\naction make() -> u64 {\n    let token = Token { amount: 1 };\n    token.amount\n}\n";
        server.open_document(uri.clone(), source.to_string());

        let definition = server.goto_definition(&uri, Position { line: 7, character: 16 }).expect("definition");
        assert_eq!(definition.range.start.line, 2);

        let refs = server.find_references(&uri, Position { line: 7, character: 16 });
        assert!(refs.len() >= 2);
    }

    #[test]
    fn test_hover() {
        let mut server = LspServer::new();
        let uri = "file:///hover.cell".to_string();
        let source = "module hover;\n\naction demo(x: u64)->u64{\n    x\n}\n";
        server.open_document(uri.clone(), source.to_string());

        let hover = server.hover(&uri, Position { line: 2, character: 7 }).expect("hover");
        assert!(hover.contents.contains("action demo"));
    }

    #[test]
    fn test_action_hover_includes_lowering_metadata() {
        let mut server = LspServer::new();
        let uri = "file:///metadata_hover.cell".to_string();
        let source = r#"
module metadata_hover

shared Config {
    threshold: u64,
}

resource Token has store, transfer, destroy {
    amount: u64,
}

action update(amount: u64) -> u64 {
    let cfg = read_ref<Config>()
    let token = create Token { amount: amount }
    consume token
    return cfg.threshold
}
"#;
        server.open_document(uri.clone(), source.to_string());

        let hover = server.hover(&uri, Position { line: 11, character: 8 }).expect("hover");
        assert!(hover.contents.contains("Lowering metadata"));
        assert!(hover.contents.contains("ELF compatible: `false`"));
        assert!(hover.contents.contains("Standalone runner compatible: `false`"));
        assert!(hover.contents.contains("Fail-closed runtime features: `none`"));
        assert!(hover.contents.contains("CKB runtime features: `consume-input-cell, read-cell-dep, verify-output-cell`"));
        assert!(hover.contents.contains("consume:Input#0"));
        assert!(hover.contents.contains("read_ref:CellDep#0"));
        assert!(hover.contents.contains("create:Output#0"));
        assert!(hover.contents.contains("Verifier obligations"));
        assert!(hover.contents.contains("cell-access:consume:Input#0 (ckb-runtime)"));
    }

    #[test]
    fn test_receipt_hover_includes_lifecycle_metadata() {
        let mut server = LspServer::new();
        let uri = "file:///lifecycle_hover.cell".to_string();
        let source = r#"
module lifecycle_hover

#[lifecycle(Created -> Active)]
receipt Ticket has store {
    state: u8,
    id: u64,
}

action activate(ticket: Ticket) -> Ticket {
    let active = 1
    consume ticket
    return create Ticket {
        state: active,
        id: ticket.id,
    }
}
"#;
        server.open_document(uri.clone(), source.to_string());

        let hover = server.hover(&uri, Position { line: 4, character: 9 }).expect("hover");
        assert!(hover.contents.contains("receipt Ticket"));
        assert!(hover.contents.contains("Lifecycle metadata"));
        assert!(hover.contents.contains("States: `Created -> Active`"));
        assert!(hover.contents.contains("Created[0] -> Active[1]"));
    }

    #[test]
    fn test_lifecycle_errors_become_lsp_diagnostics() {
        let mut server = LspServer::new();
        let uri = "file:///bad_lifecycle.cell".to_string();
        let source = r#"
module bad_lifecycle

#[lifecycle(Created -> Created)]
receipt Ticket has store {
    state: u8,
    id: u64,
}
"#;
        server.open_document(uri.clone(), source.to_string());

        let diagnostics = server.get_diagnostics(&uri);
        let error = diagnostics.iter().find(|diagnostic| diagnostic.source == "cellscript").expect("lifecycle diagnostic");
        assert_eq!(error.severity, DiagnosticSeverity::Error);
        assert!(error.message.contains("duplicate lifecycle state: Created"));
    }

    #[test]
    fn test_lowering_diagnostics_warn_for_symbolic_runtime_actions() {
        let mut server = LspServer::new();
        let uri = "file:///metadata_diagnostic.cell".to_string();
        let source = r#"
module metadata_diagnostic

shared Config {
    threshold: u64,
}

resource Token has store, transfer, destroy {
    amount: u64,
}

action update(amount: u64) -> u64 {
    let cfg = read_ref<Config>()
    let token = create Token { amount: amount }
    consume token
    return cfg.threshold
}
"#;
        server.open_document(uri.clone(), source.to_string());

        let diagnostics = server.get_diagnostics(&uri);
        let warning = diagnostics.iter().find(|diagnostic| diagnostic.source == "cellscript-lowering").expect("lowering diagnostic");
        assert_eq!(warning.severity, DiagnosticSeverity::Warning);
        assert!(warning.message.contains("not currently ELF-compatible"));
        assert!(warning.message.contains("fail-closed runtime features: none"));
        assert!(warning.message.contains("read-cell-dep"));
        assert!(warning.message.contains("consume:Input#0"));
        assert!(warning.message.contains("read_ref:CellDep#0"));
        assert!(warning.message.contains("create:Output#0"));
    }

    #[test]
    fn test_code_actions_for_lowering_diagnostics() {
        let mut server = LspServer::new();
        let uri = "file:///metadata_action.cell".to_string();
        let source = r#"
module metadata_action

shared Config {
    threshold: u64,
}

resource Token has store, transfer, destroy {
    amount: u64,
}

action update() -> u64 {
    let cfg = read_ref<Config>()
    let token = create Token { amount: cfg.threshold }
    consume token
    return cfg.threshold
}
"#;
        server.open_document(uri.clone(), source.to_string());

        let actions =
            server.code_action(&uri, Range { start: Position { line: 11, character: 0 }, end: Position { line: 11, character: 20 } });
        assert!(actions.iter().any(|action| action.title.contains("cellc metadata")));
        assert!(actions.iter().any(|action| action.title.contains("riscv64-asm")));
        assert!(actions.iter().all(|action| action.edit.is_none()));
    }

    #[test]
    fn test_format_document() {
        let mut server = LspServer::new();
        let uri = "file:///fmt.cell".to_string();
        let source = "module fmt\naction demo(x:u64)->u64{x}\n";
        server.open_document(uri.clone(), source.to_string());

        let edits = server.format_document(&uri);
        assert_eq!(edits.len(), 1);
        assert!(edits[0].new_text.contains("action demo(x: u64) -> u64 {"));
    }

    #[test]
    fn test_workspace_goto_definition_across_modules() {
        let temp = tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("Cell.toml"), "[package]\nentry = \"src/main.cell\"\n").unwrap();
        std::fs::write(root.join("src/types.cell"), "module demo::types\n\nresource Token {\n    amount: u64,\n}\n").unwrap();
        let main_source =
            "module demo::main\n\nuse demo::types::Token\n\naction inspect(token: Token) -> u64 {\n    token.amount\n}\n";
        let main_path = root.join("src/main.cell");
        std::fs::write(&main_path, main_source).unwrap();

        let mut server = LspServer::new();
        let main_uri = utf8_path_to_file_uri(&main_path);
        server.open_document(main_uri.clone(), main_source.to_string());

        let definition = server.goto_definition(&main_uri, Position { line: 4, character: 22 }).expect("cross-module definition");
        assert!(definition.uri.ends_with("/src/types.cell"));
        assert_eq!(definition.range.start.line, 2);
    }

    #[test]
    fn test_workspace_references_across_modules() {
        let temp = tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("Cell.toml"), "[package]\nentry = \"src/main.cell\"\n").unwrap();
        let types_source = "module demo::types\n\nresource Token {\n    amount: u64,\n}\n";
        let types_path = root.join("src/types.cell");
        std::fs::write(&types_path, types_source).unwrap();
        let main_source =
            "module demo::main\n\nuse demo::types::Token\n\naction inspect(token: Token) -> u64 {\n    token.amount\n}\n";
        std::fs::write(root.join("src/main.cell"), main_source).unwrap();

        let mut server = LspServer::new();
        let types_uri = utf8_path_to_file_uri(&types_path);
        server.open_document(types_uri.clone(), types_source.to_string());

        let refs = server.find_references(&types_uri, Position { line: 2, character: 10 });
        assert!(refs.iter().any(|location| location.uri.ends_with("/src/types.cell")));
        assert!(refs.iter().any(|location| location.uri.ends_with("/src/main.cell")));
        assert!(refs.len() >= 3);
    }
}
