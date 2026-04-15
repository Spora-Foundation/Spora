//! 代码格式化器
//!
//! 自动格式化 CellScript 代码

use crate::ast::*;
use crate::error::Result;

/// 格式化配置
#[derive(Debug, Clone)]
pub struct FormatConfig {
    /// 缩进宽度
    pub indent_width: usize,
    /// 使用空格而非 Tab
    pub use_spaces: bool,
    /// 最大行宽
    pub max_line_width: usize,
    /// 换行风格
    pub newline_style: NewlineStyle,
    /// 在逗号后加空格
    pub space_after_comma: bool,
    /// 在冒号后加空格
    pub space_after_colon: bool,
    /// 在操作符周围加空格
    pub spaces_around_ops: bool,
}

/// 换行风格
#[derive(Debug, Clone, Copy)]
pub enum NewlineStyle {
    Unix,    // \n
    Windows, // \r\n
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            indent_width: 4,
            use_spaces: true,
            max_line_width: 100,
            newline_style: NewlineStyle::Unix,
            space_after_comma: true,
            space_after_colon: true,
            spaces_around_ops: true,
        }
    }
}

/// 格式化器
pub struct Formatter {
    config: FormatConfig,
    output: String,
    indent_level: usize,
}

impl Formatter {
    /// 创建新的格式化器
    pub fn new(config: FormatConfig) -> Self {
        Self { config, output: String::new(), indent_level: 0 }
    }

    /// 格式化模块
    pub fn format_module(&mut self, module: &Module) -> Result<String> {
        self.output.clear();
        self.indent_level = 0;

        // 模块声明
        self.writeln(&format!("module {};", module.name));
        self.writeln("");

        // 格式化每个条目
        for (i, item) in module.items.iter().enumerate() {
            self.format_item(item)?;

            // 条目之间加空行
            if i < module.items.len() - 1 {
                self.writeln("");
            }
        }

        Ok(self.output.clone())
    }

    /// 格式化条目
    fn format_item(&mut self, item: &Item) -> Result<()> {
        match item {
            Item::Resource(r) => self.format_resource(r),
            Item::Shared(s) => self.format_shared(s),
            Item::Receipt(r) => self.format_receipt(r),
            Item::Struct(s) => self.format_struct(s),
            Item::Action(a) => self.format_action(a),
            Item::Lock(l) => self.format_lock(l),
            Item::Use(u) => self.format_use(u),
        }
    }

    /// 格式化 Resource
    fn format_resource(&mut self, resource: &ResourceDef) -> Result<()> {
        // 能力属性
        if !resource.capabilities.is_empty() {
            let caps: Vec<String> = resource.capabilities.iter().map(|c| format!("{:?}", c).to_lowercase()).collect();
            self.writeln(&format!("#[capability({})]", caps.join(", ")));
        }

        // 资源声明
        self.write(&format!("resource {} {{", resource.name));

        if resource.fields.is_empty() {
            self.write("}");
        } else {
            self.writeln("");
            self.indent_level += 1;

            for field in &resource.fields {
                self.write_indent();
                self.write(&format!("{}: {:?},", field.name, field.ty));
                self.writeln("");
            }

            self.indent_level -= 1;
            self.write_indent();
            self.write("}");
        }

        self.writeln("");
        Ok(())
    }

    /// 格式化 Shared
    fn format_shared(&mut self, shared: &SharedDef) -> Result<()> {
        self.write(&format!("shared {} {{", shared.name));

        if shared.fields.is_empty() {
            self.write("}");
        } else {
            self.writeln("");
            self.indent_level += 1;

            for field in &shared.fields {
                self.write_indent();
                self.write(&format!("{}: {:?},", field.name, field.ty));
                self.writeln("");
            }

            self.indent_level -= 1;
            self.write_indent();
            self.write("}");
        }

        self.writeln("");
        Ok(())
    }

    /// 格式化 Receipt
    fn format_receipt(&mut self, receipt: &ReceiptDef) -> Result<()> {
        // 生命周期属性
        if let Some(lifecycle) = &receipt.lifecycle {
            self.writeln(&format!("#[lifecycle({})]", lifecycle.states.join(", ")));
        }

        self.write(&format!("receipt {} {{", receipt.name));

        if receipt.fields.is_empty() {
            self.write("}");
        } else {
            self.writeln("");
            self.indent_level += 1;

            for field in &receipt.fields {
                self.write_indent();
                self.write(&format!("{}: {:?},", field.name, field.ty));
                self.writeln("");
            }

            self.indent_level -= 1;
            self.write_indent();
            self.write("}");
        }

        self.writeln("");
        Ok(())
    }

    /// 格式化 Struct
    fn format_struct(&mut self, struct_def: &StructDef) -> Result<()> {
        self.write(&format!("struct {} {{", struct_def.name));

        if struct_def.fields.is_empty() {
            self.write("}");
        } else {
            self.writeln("");
            self.indent_level += 1;

            for field in &struct_def.fields {
                self.write_indent();
                self.write(&format!("{}: {:?},", field.name, field.ty));
                self.writeln("");
            }

            self.indent_level -= 1;
            self.write_indent();
            self.write("}");
        }

        self.writeln("");
        Ok(())
    }

    /// 格式化 Action
    fn format_action(&mut self, action: &ActionDef) -> Result<()> {
        // 效果属性
        self.writeln(&format!("#[effect({:?})]", action.effect));

        // 调度器提示
        if let Some(hint) = &action.scheduler_hint {
            self.writeln(&format!(
                "#[scheduler_hint({}, estimated_cycles = {})]",
                if hint.parallelizable { "parallel" } else { "sequential" },
                hint.estimated_cycles
            ));
        }

        // Action 签名
        self.write(&format!("action {}", action.name));

        // 参数
        self.write("(");
        for (i, param) in action.params.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.write(&format!("{}: {:?}", param.name, param.ty));
        }
        self.write(")");

        // 返回类型
        if let Some(ret) = &action.return_type {
            self.write(&format!(" -> {:?}", ret));
        }

        // 函数体
        self.writeln(" {");
        self.indent_level += 1;

        for stmt in &action.body {
            self.format_stmt(stmt)?;
        }

        self.indent_level -= 1;
        self.write_indent();
        self.writeln("}");

        Ok(())
    }

    /// 格式化 Lock
    fn format_lock(&mut self, lock: &LockDef) -> Result<()> {
        self.write(&format!("lock {}", lock.name));

        // 参数
        self.write("(");
        for (i, param) in lock.params.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.write(&format!("{}: {:?}", param.name, param.ty));
        }
        self.write(")");

        // 返回类型
        self.write(&format!(" -> {:?}", lock.return_type));

        // 函数体
        self.writeln(" {");
        self.indent_level += 1;

        self.format_expr(&lock.body)?;

        self.indent_level -= 1;
        self.write_indent();
        self.writeln("}");

        Ok(())
    }

    /// 格式化 Use
    fn format_use(&mut self, use_stmt: &UseStmt) -> Result<()> {
        let module_path = use_stmt.module_path.join("::");
        if use_stmt.imports.len() == 1 {
            let import = &use_stmt.imports[0];
            let path = if module_path.is_empty() {
                import.name.clone()
            } else {
                format!("{}::{}", module_path, import.name)
            };
            if let Some(alias) = &import.alias {
                self.writeln(&format!("use {} as {};", path, alias));
            } else {
                self.writeln(&format!("use {};", path));
            }
        } else {
            let imports = use_stmt
                .imports
                .iter()
                .map(|import| match &import.alias {
                    Some(alias) => format!("{} as {}", import.name, alias),
                    None => import.name.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            self.writeln(&format!("use {}::{{{}}};", module_path, imports));
        }
        Ok(())
    }

    /// 格式化语句
    fn format_stmt(&mut self, stmt: &Stmt) -> Result<()> {
        self.write_indent();

        match stmt {
            Stmt::Let(let_stmt) => {
                self.write(&format!("let {}", let_stmt.name));
                if let Some(ty) = &let_stmt.ty {
                    self.write(&format!(": {:?}", ty));
                }
                self.write(" = ");
                self.format_expr(&let_stmt.value)?;
                self.writeln(";");
            }
            Stmt::Expr(expr) => {
                self.format_expr(expr)?;
                self.writeln(";");
            }
            Stmt::If(if_stmt) => {
                self.write("if ");
                self.format_expr(&if_stmt.condition)?;
                self.writeln(" {");
                self.indent_level += 1;
                for stmt in &if_stmt.then_branch {
                    self.format_stmt(stmt)?;
                }
                self.indent_level -= 1;
                self.write_indent();
                self.write("}");

                if let Some(else_branch) = &if_stmt.else_branch {
                    self.writeln(" else {");
                    self.indent_level += 1;
                    for stmt in else_branch {
                        self.format_stmt(stmt)?;
                    }
                    self.indent_level -= 1;
                    self.write_indent();
                    self.writeln("}");
                } else {
                    self.writeln("");
                }
            }
            Stmt::For(for_stmt) => {
                self.write(&format!("for {} in ", for_stmt.var));
                self.format_expr(&for_stmt.iterable)?;
                self.writeln(" {");
                self.indent_level += 1;
                for stmt in &for_stmt.body {
                    self.format_stmt(stmt)?;
                }
                self.indent_level -= 1;
                self.write_indent();
                self.writeln("}");
            }
            Stmt::While(while_stmt) => {
                self.write("while ");
                self.format_expr(&while_stmt.condition)?;
                self.writeln(" {");
                self.indent_level += 1;
                for stmt in &while_stmt.body {
                    self.format_stmt(stmt)?;
                }
                self.indent_level -= 1;
                self.write_indent();
                self.writeln("}");
            }
            Stmt::Return(expr) => {
                self.write("return");
                if let Some(e) = expr {
                    self.write(" ");
                    self.format_expr(e)?;
                }
                self.writeln(";");
            }
            Stmt::Break => {
                self.writeln("break;");
            }
            Stmt::Continue => {
                self.writeln("continue;");
            }
            Stmt::Block(stmts) => {
                self.writeln("{");
                self.indent_level += 1;
                for stmt in stmts {
                    self.format_stmt(stmt)?;
                }
                self.indent_level -= 1;
                self.write_indent();
                self.writeln("}");
            }
        }

        Ok(())
    }

    /// 格式化表达式
    fn format_expr(&mut self, expr: &Expr) -> Result<()> {
        match expr {
            Expr::Literal(lit) => {
                self.write(&format!("{:?}", lit));
            }
            Expr::Path(path) => {
                self.write(&path.name);
            }
            Expr::Binary(bin) => {
                self.format_expr(&bin.left)?;
                if self.config.spaces_around_ops {
                    self.write(&format!(" {:?} ", bin.op));
                } else {
                    self.write(&format!("{:?}", bin.op));
                }
                self.format_expr(&bin.right)?;
            }
            Expr::Unary(unary) => {
                self.write(&format!("{:?}", unary.op));
                self.format_expr(&unary.expr)?;
            }
            Expr::Call(call) => {
                self.write(&call.func);
                self.write("(");
                for (i, arg) in call.args.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expr(arg)?;
                }
                self.write(")");
            }
            Expr::FieldAccess(field) => {
                self.format_expr(&field.expr)?;
                self.write(&format!(".{}", field.field));
            }
            Expr::Index(index) => {
                self.format_expr(&index.expr)?;
                self.write("[");
                self.format_expr(&index.index)?;
                self.write("]");
            }
            Expr::Create(create) => {
                self.write(&format!("create {} {{", create.resource_type));
                for (i, (name, value)) in create.fields.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.write(&format!("{}: ", name));
                    self.format_expr(value)?;
                }
                self.write("}");
            }
            Expr::Destroy(destroy) => {
                self.write("destroy ");
                self.format_expr(&destroy.expr)?;
            }
            Expr::Transfer(transfer) => {
                self.write("transfer ");
                self.format_expr(&transfer.expr)?;
                self.write(" to ");
                self.format_expr(&transfer.to)?;
            }
            Expr::Tuple(elems) => {
                self.write("(");
                for (i, elem) in elems.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expr(elem)?;
                }
                self.write(")");
            }
            Expr::Array(elems) => {
                self.write("[");
                for (i, elem) in elems.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expr(elem)?;
                }
                self.write("]");
            }
            Expr::Block(stmts) => {
                self.writeln("{");
                self.indent_level += 1;
                for stmt in stmts {
                    self.format_stmt(stmt)?;
                }
                self.indent_level -= 1;
                self.write_indent();
                self.write("}");
            }
            Expr::If(if_expr) => {
                self.write("if ");
                self.format_expr(&if_expr.condition)?;
                self.writeln(" {");
                self.indent_level += 1;
                for stmt in &if_expr.then_branch {
                    self.format_stmt(stmt)?;
                }
                self.indent_level -= 1;
                self.write_indent();
                self.write("}");

                if let Some(else_branch) = &if_expr.else_branch {
                    self.writeln(" else {");
                    self.indent_level += 1;
                    for stmt in else_branch {
                        self.format_stmt(stmt)?;
                    }
                    self.indent_level -= 1;
                    self.write_indent();
                    self.write("}");
                }
            }
            Expr::Match(match_expr) => {
                self.write("match ");
                self.format_expr(&match_expr.expr)?;
                self.writeln(" {");
                self.indent_level += 1;
                for arm in &match_expr.arms {
                    self.write_indent();
                    self.format_pattern(&arm.pattern)?;
                    self.write(" => ");
                    self.format_expr(&arm.expr)?;
                    self.writeln(",");
                }
                self.indent_level -= 1;
                self.write_indent();
                self.write("}");
            }
            Expr::Assert(assert_expr) => {
                self.write("assert!(");
                self.format_expr(&assert_expr.condition)?;
                if let Some(msg) = &assert_expr.message {
                    self.write(&format!(", \"{}\"", msg));
                }
                self.write(")");
            }
        }

        Ok(())
    }

    /// 格式化模式
    fn format_pattern(&mut self, pattern: &Pattern) -> Result<()> {
        match pattern {
            Pattern::Wildcard => self.write("_"),
            Pattern::Literal(lit) => self.write(&format!("{:?}", lit)),
            Pattern::Path(path) => self.write(&path.name),
            Pattern::Struct { name, fields } => {
                self.write(&format!("{} {{ ", name));
                for (i, (field_name, field_pattern)) in fields.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.write(&format!("{}: ", field_name));
                    self.format_pattern(field_pattern)?;
                }
                self.write(" }");
            }
            Pattern::Tuple(patterns) => {
                self.write("(");
                for (i, p) in patterns.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_pattern(p)?;
                }
                self.write(")");
            }
        }
        Ok(())
    }

    /// 写入缩进
    fn write_indent(&mut self) {
        let indent = if self.config.use_spaces {
            " ".repeat(self.config.indent_width * self.indent_level)
        } else {
            "\t".repeat(self.indent_level)
        };
        self.output.push_str(&indent);
    }

    /// 写入文本
    fn write(&mut self, s: &str) {
        self.output.push_str(s);
    }

    /// 写入文本并换行
    fn writeln(&mut self, s: &str) {
        self.output.push_str(s);
        match self.config.newline_style {
            NewlineStyle::Unix => self.output.push('\n'),
            NewlineStyle::Windows => self.output.push_str("\r\n"),
        }
    }
}

/// 格式化代码
pub fn format(module: &Module, config: FormatConfig) -> Result<String> {
    let mut formatter = Formatter::new(config);
    formatter.format_module(module)
}

/// 使用默认配置格式化
pub fn format_default(module: &Module) -> Result<String> {
    format(module, FormatConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formatter() {
        let module = Module {
            name: "test".to_string(),
            items: vec![Item::Resource(ResourceDef {
                name: "Token".to_string(),
                capabilities: vec![Capability::Store],
                fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: crate::error::Span::default() }],
                span: crate::error::Span::default(),
            })],
            span: crate::error::Span::default(),
        };

        let formatted = format_default(&module).unwrap();
        assert!(formatted.contains("module test;"));
        assert!(formatted.contains("resource Token"));
    }
}
