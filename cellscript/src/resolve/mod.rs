//! 名称解析和模块系统
//!
//! 处理模块导入、符号解析、路径解析

use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use std::collections::HashMap;

/// 模块解析器
pub struct ModuleResolver {
    /// 已加载的模块
    modules: HashMap<String, Module>,
    /// 当前模块的符号表
    symbol_tables: HashMap<String, SymbolTable>,
    /// 导入映射
    imports: HashMap<String, Vec<ImportItem>>,
}

/// 符号表
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    /// 类型定义
    types: HashMap<String, TypeDef>,
    /// 函数/Action
    functions: HashMap<String, FunctionDef>,
    /// 常量
    constants: HashMap<String, ConstantDef>,
    /// 导入的符号
    imported: HashMap<String, String>, // 本地名 -> 完全限定名
}

/// 类型定义
#[derive(Debug, Clone)]
pub enum TypeDef {
    Resource(ResourceDef),
    Shared(SharedDef),
    Receipt(ReceiptDef),
    Struct(StructDef),
    Enum(EnumDef),
}

/// 函数定义
#[derive(Debug, Clone)]
pub enum FunctionDef {
    Action(ActionDef),
    Function(FnDef),
    Lock(LockDef),
}

/// 常量定义
#[derive(Debug, Clone)]
pub struct ConstantDef {
    pub name: String,
    pub ty: Type,
    pub value: Expr,
}

/// 导入项
#[derive(Debug, Clone)]
pub struct ImportItem {
    pub module_path: Vec<String>,
    pub name: String,
    pub alias: Option<String>,
    pub span: Span,
}

impl ModuleResolver {
    /// 创建新的模块解析器
    pub fn new() -> Self {
        Self { modules: HashMap::new(), symbol_tables: HashMap::new(), imports: HashMap::new() }
    }

    /// 注册模块
    pub fn register_module(&mut self, module: Module) -> Result<()> {
        let name = module.name.clone();
        if self.modules.contains_key(&name) {
            return Err(CompileError::new(format!("duplicate module '{}'", name), module.span));
        }

        // 构建符号表
        let mut symbol_table = SymbolTable::default();

        for item in &module.items {
            match item {
                Item::Resource(r) => {
                    symbol_table.types.insert(r.name.clone(), TypeDef::Resource(r.clone()));
                }
                Item::Shared(s) => {
                    symbol_table.types.insert(s.name.clone(), TypeDef::Shared(s.clone()));
                }
                Item::Receipt(r) => {
                    symbol_table.types.insert(r.name.clone(), TypeDef::Receipt(r.clone()));
                }
                Item::Struct(s) => {
                    symbol_table.types.insert(s.name.clone(), TypeDef::Struct(s.clone()));
                }
                Item::Enum(e) => {
                    symbol_table.types.insert(e.name.clone(), TypeDef::Enum(e.clone()));
                }
                Item::Const(c) => {
                    symbol_table
                        .constants
                        .insert(c.name.clone(), ConstantDef { name: c.name.clone(), ty: c.ty.clone(), value: c.value.clone() });
                }
                Item::Action(a) => {
                    symbol_table.functions.insert(a.name.clone(), FunctionDef::Action(a.clone()));
                }
                Item::Function(f) => {
                    symbol_table.functions.insert(f.name.clone(), FunctionDef::Function(f.clone()));
                }
                Item::Lock(l) => {
                    symbol_table.functions.insert(l.name.clone(), FunctionDef::Lock(l.clone()));
                }
                Item::Use(u) => {
                    for import in &u.imports {
                        let import_item = ImportItem {
                            module_path: u.module_path.clone(),
                            name: import.name.clone(),
                            alias: import.alias.clone(),
                            span: u.span,
                        };

                        self.process_import(&mut symbol_table, &import_item)?;
                        self.imports.entry(name.clone()).or_default().push(import_item);
                    }
                }
            }
        }

        self.symbol_tables.insert(name.clone(), symbol_table);
        self.modules.insert(name, module);

        Ok(())
    }

    /// 处理导入
    fn process_import(&mut self, symbol_table: &mut SymbolTable, import: &ImportItem) -> Result<()> {
        if import.module_path.is_empty() || import.name.is_empty() {
            return Err(CompileError::new("empty import path", import.span));
        }

        let full_path = import.module_path.iter().chain(std::iter::once(&import.name)).cloned().collect::<Vec<_>>().join("::");
        let local_name = import.alias.clone().unwrap_or_else(|| import.name.clone());

        symbol_table.imported.insert(local_name, full_path);

        Ok(())
    }

    /// 解析类型引用
    pub fn resolve_type(&self, module: &str, name: &str) -> Option<TypeDef> {
        if let Some((target_module, symbol)) = name.rsplit_once("::") {
            if let Some(table) = self.symbol_tables.get(target_module) {
                return table.types.get(symbol).cloned();
            }
        }

        // 先查当前模块
        if let Some(table) = self.symbol_tables.get(module) {
            // 查本地类型
            if let Some(ty) = table.types.get(name) {
                return Some(ty.clone());
            }

            // 查导入的类型
            if let Some(full_path) = table.imported.get(name) {
                // 解析完全限定路径
                let parts: Vec<&str> = full_path.split("::").collect();
                if let Some(type_name) = parts.last() {
                    // 查找目标模块
                    for (mod_name, table) in &self.symbol_tables {
                        if full_path.starts_with(mod_name) {
                            return table.types.get(*type_name).cloned();
                        }
                    }
                }
            }
        }

        self.resolve_type_global(name)
    }

    /// 解析函数引用
    pub fn resolve_function(&self, module: &str, name: &str) -> Option<FunctionDef> {
        if let Some((target_module, symbol)) = name.rsplit_once("::") {
            if let Some(table) = self.symbol_tables.get(target_module) {
                return table.functions.get(symbol).cloned();
            }
        }

        if let Some(table) = self.symbol_tables.get(module) {
            // 查本地函数
            if let Some(func) = table.functions.get(name) {
                return Some(func.clone());
            }

            // 查导入的函数
            if let Some(full_path) = table.imported.get(name) {
                let parts: Vec<&str> = full_path.split("::").collect();
                if let Some(func_name) = parts.last() {
                    for (mod_name, table) in &self.symbol_tables {
                        if full_path.starts_with(mod_name) {
                            return table.functions.get(*func_name).cloned();
                        }
                    }
                }
            }
        }

        self.resolve_function_global(name)
    }

    pub fn resolve_constant(&self, module: &str, name: &str) -> Option<ConstantDef> {
        if let Some((target_module, symbol)) = name.rsplit_once("::") {
            if let Some(table) = self.symbol_tables.get(target_module) {
                return table.constants.get(symbol).cloned();
            }
        }

        if let Some(table) = self.symbol_tables.get(module) {
            if let Some(constant) = table.constants.get(name) {
                return Some(constant.clone());
            }

            if let Some(full_path) = table.imported.get(name) {
                if let Some((target_module, symbol)) = full_path.rsplit_once("::") {
                    if let Some(target_table) = self.symbol_tables.get(target_module) {
                        return target_table.constants.get(symbol).cloned();
                    }
                }
            }
        }

        self.resolve_constant_global(name)
    }

    pub fn resolve_type_global(&self, name: &str) -> Option<TypeDef> {
        let symbol = name.rsplit("::").next().unwrap_or(name);
        self.symbol_tables.values().find_map(|table| table.types.get(symbol).cloned())
    }

    pub fn resolve_function_global(&self, name: &str) -> Option<FunctionDef> {
        let symbol = name.rsplit("::").next().unwrap_or(name);
        self.symbol_tables.values().find_map(|table| table.functions.get(symbol).cloned())
    }

    pub fn resolve_constant_global(&self, name: &str) -> Option<ConstantDef> {
        let symbol = name.rsplit("::").next().unwrap_or(name);
        self.symbol_tables.values().find_map(|table| table.constants.get(symbol).cloned())
    }

    pub fn type_is_linear(&self, module: &str, name: &str) -> bool {
        matches!(self.resolve_type(module, name), Some(TypeDef::Resource(_)) | Some(TypeDef::Shared(_)) | Some(TypeDef::Receipt(_)))
    }

    pub fn type_fields(&self, module: &str, name: &str) -> Option<Vec<(String, Type)>> {
        match self.resolve_type(module, name)? {
            TypeDef::Resource(resource) => Some(resource.fields.into_iter().map(|field| (field.name, field.ty)).collect()),
            TypeDef::Shared(shared) => Some(shared.fields.into_iter().map(|field| (field.name, field.ty)).collect()),
            TypeDef::Receipt(receipt) => Some(receipt.fields.into_iter().map(|field| (field.name, field.ty)).collect()),
            TypeDef::Struct(struct_def) => Some(struct_def.fields.into_iter().map(|field| (field.name, field.ty)).collect()),
            TypeDef::Enum(_) => None,
        }
    }

    /// 获取模块的公开符号
    pub fn get_public_symbols(&self, module: &str) -> Vec<String> {
        let mut symbols = Vec::new();

        if let Some(table) = self.symbol_tables.get(module) {
            for name in table.types.keys() {
                symbols.push(name.clone());
            }
            for name in table.functions.keys() {
                symbols.push(name.clone());
            }
        }

        symbols
    }

    /// 检查循环依赖
    pub fn check_circular_deps(&self) -> Result<()> {
        // 简化实现：检查导入的模块是否存在
        for (_module_name, imports) in &self.imports {
            for import in imports {
                let target_module = import.module_path.join("::");
                if !self.modules.contains_key(&target_module) && !target_module.starts_with("spora::") {
                    return Err(CompileError::new(format!("module '{}' not found", target_module), import.span));
                }
            }
        }

        Ok(())
    }

    /// 解析完全限定名
    pub fn resolve_qualified_name(&self, path: &[String]) -> Option<ResolvedName> {
        if path.is_empty() {
            return None;
        }

        // 第一个组件是模块名
        let module_name = &path[0];

        if let Some(table) = self.symbol_tables.get(module_name) {
            if path.len() == 1 {
                // 只有模块名
                return Some(ResolvedName::Module(module_name.clone()));
            }

            // 第二个组件是符号名
            let symbol_name = &path[1];

            if let Some(ty) = table.types.get(symbol_name) {
                return Some(ResolvedName::Type(module_name.clone(), symbol_name.clone(), ty.clone()));
            }

            if let Some(func) = table.functions.get(symbol_name) {
                return Some(ResolvedName::Function(module_name.clone(), symbol_name.clone(), func.clone()));
            }
        }

        None
    }
}

/// 解析后的名称
#[derive(Debug, Clone)]
pub enum ResolvedName {
    Module(String),
    Type(String, String, TypeDef),         // 模块, 名称, 定义
    Function(String, String, FunctionDef), // 模块, 名称, 定义
}

/// 路径解析器
pub struct PathResolver;

impl PathResolver {
    /// 解析路径字符串
    pub fn parse_path(path: &str) -> Vec<String> {
        path.split("::").map(|s| s.to_string()).collect()
    }

    /// 构建完全限定名
    pub fn build_qualified_name(module: &str, name: &str) -> String {
        format!("{}::{}", module, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_resolver() {
        let mut resolver = ModuleResolver::new();

        // 创建一个测试模块
        let module = Module {
            name: "test".to_string(),
            items: vec![Item::Resource(ResourceDef {
                name: "Token".to_string(),
                capabilities: vec![Capability::Store],
                fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: Span::default() }],
                span: Span::default(),
            })],
            span: Span::default(),
        };

        resolver.register_module(module).unwrap();

        // 测试类型解析
        let ty = resolver.resolve_type("test", "Token");
        assert!(ty.is_some());
    }

    #[test]
    fn test_grouped_use_resolves_multiple_symbols() {
        let mut resolver = ModuleResolver::new();

        resolver
            .register_module(Module {
                name: "spora::fungible_token".to_string(),
                items: vec![
                    Item::Resource(ResourceDef {
                        name: "Token".to_string(),
                        capabilities: vec![Capability::Store],
                        fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: Span::default() }],
                        span: Span::default(),
                    }),
                    Item::Resource(ResourceDef {
                        name: "MintAuthority".to_string(),
                        capabilities: vec![Capability::Store],
                        fields: vec![Field { name: "max_supply".to_string(), ty: Type::U64, span: Span::default() }],
                        span: Span::default(),
                    }),
                ],
                span: Span::default(),
            })
            .unwrap();

        resolver
            .register_module(Module {
                name: "spora::launch".to_string(),
                items: vec![Item::Use(UseStmt {
                    module_path: vec!["spora".to_string(), "fungible_token".to_string()],
                    imports: vec![
                        UseImport { name: "Token".to_string(), alias: None },
                        UseImport { name: "MintAuthority".to_string(), alias: None },
                    ],
                    span: Span::default(),
                })],
                span: Span::default(),
            })
            .unwrap();

        assert!(matches!(resolver.resolve_type("spora::launch", "Token"), Some(TypeDef::Resource(_))));
        assert!(matches!(resolver.resolve_type("spora::launch", "MintAuthority"), Some(TypeDef::Resource(_))));
    }

    #[test]
    fn test_path_resolver() {
        let path = PathResolver::parse_path("spora::fungible_token::Token");
        assert_eq!(path, vec!["spora", "fungible_token", "Token"]);

        let qualified = PathResolver::build_qualified_name("spora", "Token");
        assert_eq!(qualified, "spora::Token");
    }
}
