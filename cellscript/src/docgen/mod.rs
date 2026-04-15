//! 文档生成器
//!
//! 从 CellScript 源代码提取文档并生成 HTML/Markdown

use crate::ast::*;
use crate::error::Result;
use std::collections::HashMap;
use std::fmt::Write;

/// 文档生成器
pub struct DocGenerator {
    /// 模块文档
    modules: Vec<ModuleDoc>,
    /// 输出格式
    format: OutputFormat,
}

/// 输出格式
#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Html,
    Markdown,
    Json,
}

/// 模块文档
#[derive(Debug, Clone)]
pub struct ModuleDoc {
    pub name: String,
    pub description: String,
    pub items: Vec<ItemDoc>,
}

/// 条目文档
#[derive(Debug, Clone)]
pub enum ItemDoc {
    Resource(ResourceDoc),
    Shared(SharedDoc),
    Receipt(ReceiptDoc),
    Struct(StructDoc),
    Action(ActionDoc),
    Lock(LockDoc),
    Constant(ConstantDoc),
}

/// Resource 文档
#[derive(Debug, Clone)]
pub struct ResourceDoc {
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub fields: Vec<FieldDoc>,
    pub examples: Vec<String>,
}

/// Shared 文档
#[derive(Debug, Clone)]
pub struct SharedDoc {
    pub name: String,
    pub description: String,
    pub fields: Vec<FieldDoc>,
    pub examples: Vec<String>,
}

/// Receipt 文档
#[derive(Debug, Clone)]
pub struct ReceiptDoc {
    pub name: String,
    pub description: String,
    pub lifecycle: Option<Vec<String>>,
    pub fields: Vec<FieldDoc>,
    pub examples: Vec<String>,
}

/// Struct 文档
#[derive(Debug, Clone)]
pub struct StructDoc {
    pub name: String,
    pub description: String,
    pub fields: Vec<FieldDoc>,
}

/// Action 文档
#[derive(Debug, Clone)]
pub struct ActionDoc {
    pub name: String,
    pub description: String,
    pub effect_class: String,
    pub scheduler_hint: Option<SchedulerHintDoc>,
    pub parameters: Vec<ParamDoc>,
    pub return_type: Option<String>,
    pub examples: Vec<String>,
}

/// Lock 文档
#[derive(Debug, Clone)]
pub struct LockDoc {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ParamDoc>,
    pub return_type: String,
}

/// 常量文档
#[derive(Debug, Clone)]
pub struct ConstantDoc {
    pub name: String,
    pub description: String,
    pub ty: String,
    pub value: String,
}

/// 字段文档
#[derive(Debug, Clone)]
pub struct FieldDoc {
    pub name: String,
    pub ty: String,
    pub description: String,
}

/// 参数文档
#[derive(Debug, Clone)]
pub struct ParamDoc {
    pub name: String,
    pub ty: String,
    pub description: String,
}

/// 调度器提示文档
#[derive(Debug, Clone)]
pub struct SchedulerHintDoc {
    pub parallelizable: bool,
    pub estimated_cycles: u64,
}

impl DocGenerator {
    /// 创建新的文档生成器
    pub fn new(format: OutputFormat) -> Self {
        Self { modules: Vec::new(), format }
    }

    /// 添加模块
    pub fn add_module(&mut self, module: &Module) {
        let mut module_doc = ModuleDoc { name: module.name.clone(), description: extract_module_doc(module), items: Vec::new() };

        for item in &module.items {
            if let Some(doc) = self.extract_item_doc(item) {
                module_doc.items.push(doc);
            }
        }

        self.modules.push(module_doc);
    }

    /// 提取条目文档
    fn extract_item_doc(&self, item: &Item) -> Option<ItemDoc> {
        match item {
            Item::Resource(r) => Some(ItemDoc::Resource(self.extract_resource_doc(r))),
            Item::Shared(s) => Some(ItemDoc::Shared(self.extract_shared_doc(s))),
            Item::Receipt(r) => Some(ItemDoc::Receipt(self.extract_receipt_doc(r))),
            Item::Struct(s) => Some(ItemDoc::Struct(self.extract_struct_doc(s))),
            Item::Action(a) => Some(ItemDoc::Action(self.extract_action_doc(a))),
            Item::Lock(l) => Some(ItemDoc::Lock(self.extract_lock_doc(l))),
            _ => None,
        }
    }

    /// 提取 Resource 文档
    fn extract_resource_doc(&self, resource: &ResourceDef) -> ResourceDoc {
        ResourceDoc {
            name: resource.name.clone(),
            description: format!("Resource type with {} capabilities", resource.capabilities.len()),
            capabilities: resource.capabilities.iter().map(|c| format!("{:?}", c)).collect(),
            fields: resource
                .fields
                .iter()
                .map(|f| FieldDoc { name: f.name.clone(), ty: format!("{:?}", f.ty), description: String::new() })
                .collect(),
            examples: Vec::new(),
        }
    }

    /// 提取 Shared 文档
    fn extract_shared_doc(&self, shared: &SharedDef) -> SharedDoc {
        SharedDoc {
            name: shared.name.clone(),
            description: "Shared state resource".to_string(),
            fields: shared
                .fields
                .iter()
                .map(|f| FieldDoc { name: f.name.clone(), ty: format!("{:?}", f.ty), description: String::new() })
                .collect(),
            examples: Vec::new(),
        }
    }

    /// 提取 Receipt 文档
    fn extract_receipt_doc(&self, receipt: &ReceiptDef) -> ReceiptDoc {
        ReceiptDoc {
            name: receipt.name.clone(),
            description: "Receipt with lifecycle".to_string(),
            lifecycle: receipt.lifecycle.as_ref().map(|l| l.states.clone()),
            fields: receipt
                .fields
                .iter()
                .map(|f| FieldDoc { name: f.name.clone(), ty: format!("{:?}", f.ty), description: String::new() })
                .collect(),
            examples: Vec::new(),
        }
    }

    /// 提取 Struct 文档
    fn extract_struct_doc(&self, struct_def: &StructDef) -> StructDoc {
        StructDoc {
            name: struct_def.name.clone(),
            description: "Struct definition".to_string(),
            fields: struct_def
                .fields
                .iter()
                .map(|f| FieldDoc { name: f.name.clone(), ty: format!("{:?}", f.ty), description: String::new() })
                .collect(),
        }
    }

    /// 提取 Action 文档
    fn extract_action_doc(&self, action: &ActionDef) -> ActionDoc {
        ActionDoc {
            name: action.name.clone(),
            description: action.doc_comment.clone().unwrap_or_default(),
            effect_class: format!("{:?}", action.effect),
            scheduler_hint: action
                .scheduler_hint
                .as_ref()
                .map(|h| SchedulerHintDoc { parallelizable: h.parallelizable, estimated_cycles: h.estimated_cycles }),
            parameters: action
                .params
                .iter()
                .map(|p| ParamDoc { name: p.name.clone(), ty: format!("{:?}", p.ty), description: String::new() })
                .collect(),
            return_type: action.return_type.as_ref().map(|t| format!("{:?}", t)),
            examples: Vec::new(),
        }
    }

    /// 提取 Lock 文档
    fn extract_lock_doc(&self, lock: &LockDef) -> LockDoc {
        LockDoc {
            name: lock.name.clone(),
            description: "Lock definition".to_string(),
            parameters: lock
                .params
                .iter()
                .map(|p| ParamDoc { name: p.name.clone(), ty: format!("{:?}", p.ty), description: String::new() })
                .collect(),
            return_type: format!("{:?}", lock.return_type),
        }
    }

    /// 生成文档
    pub fn generate(&self) -> String {
        match self.format {
            OutputFormat::Html => self.generate_html(),
            OutputFormat::Markdown => self.generate_markdown(),
            OutputFormat::Json => self.generate_json(),
        }
    }

    /// 生成 HTML 文档
    fn generate_html(&self) -> String {
        let mut html = String::new();

        html.push_str("<!DOCTYPE html>\n");
        html.push_str("<html>\n<head>\n");
        html.push_str("<meta charset=\"UTF-8\">\n");
        html.push_str("<title>CellScript API Documentation</title>\n");
        html.push_str(&self.generate_css());
        html.push_str("</head>\n<body>\n");
        html.push_str("<div class=\"container\">\n");
        html.push_str("<h1>CellScript API Documentation</h1>\n");

        for module in &self.modules {
            html.push_str(&self.generate_module_html(module));
        }

        html.push_str("</div>\n</body>\n</html>");
        html
    }

    /// 生成 CSS
    fn generate_css(&self) -> String {
        r#"<style>
            body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; max-width: 1200px; margin: 0 auto; padding: 20px; }
            h1 { color: #2c3e50; border-bottom: 2px solid #3498db; padding-bottom: 10px; }
            h2 { color: #34495e; margin-top: 30px; }
            h3 { color: #7f8c8d; }
            .module { background: #f8f9fa; padding: 20px; border-radius: 8px; margin: 20px 0; }
            .item { background: white; padding: 15px; margin: 10px 0; border-left: 4px solid #3498db; border-radius: 4px; }
            .resource { border-left-color: #e74c3c; }
            .shared { border-left-color: #f39c12; }
            .receipt { border-left-color: #9b59b6; }
            .action { border-left-color: #27ae60; }
            .lock { border-left-color: #16a085; }
            .field { margin: 5px 0; padding: 5px; background: #ecf0f1; border-radius: 3px; }
            .param { margin: 5px 0; }
            .effect-class { display: inline-block; padding: 2px 8px; border-radius: 12px; font-size: 12px; font-weight: bold; }
            .effect-creating { background: #2ecc71; color: white; }
            .effect-mutating { background: #f39c12; color: white; }
            .effect-destroying { background: #e74c3c; color: white; }
            .effect-pure { background: #95a5a6; color: white; }
            .capabilities { margin: 10px 0; }
            .capability { display: inline-block; margin: 2px; padding: 2px 8px; background: #ecf0f1; border-radius: 12px; font-size: 12px; }
            .lifecycle { margin: 10px 0; }
            .lifecycle-state { display: inline-block; margin: 2px; padding: 2px 8px; background: #e8f4f8; border-radius: 12px; font-size: 12px; }
            .scheduler-hint { margin: 10px 0; padding: 10px; background: #fff3cd; border-radius: 4px; }
            code { background: #f4f4f4; padding: 2px 6px; border-radius: 3px; font-family: 'Consolas', monospace; }
            pre { background: #f4f4f4; padding: 15px; border-radius: 4px; overflow-x: auto; }
        </style>"#.to_string()
    }

    /// 生成模块 HTML
    fn generate_module_html(&self, module: &ModuleDoc) -> String {
        let mut html = String::new();

        html.push_str(&format!(
            r#"<div class="module">
            <h2>Module: {}</h2>
            <p>{}</p>
        "#,
            module.name, module.description
        ));

        for item in &module.items {
            html.push_str(&self.generate_item_html(item));
        }

        html.push_str("</div>\n");
        html
    }

    /// 生成条目 HTML
    fn generate_item_html(&self, item: &ItemDoc) -> String {
        match item {
            ItemDoc::Resource(r) => self.generate_resource_html(r),
            ItemDoc::Shared(s) => self.generate_shared_html(s),
            ItemDoc::Receipt(r) => self.generate_receipt_html(r),
            ItemDoc::Struct(s) => self.generate_struct_html(s),
            ItemDoc::Action(a) => self.generate_action_html(a),
            ItemDoc::Lock(l) => self.generate_lock_html(l),
            ItemDoc::Constant(c) => self.generate_constant_html(c),
        }
    }

    /// 生成 Resource HTML
    fn generate_resource_html(&self, resource: &ResourceDoc) -> String {
        let mut html = String::new();

        html.push_str(&format!(
            r#"<div class="item resource">
            <h3>resource {}</h3>
            <p>{}</p>
            <div class="capabilities">
                <strong>Capabilities:</strong>
        "#,
            resource.name, resource.description
        ));

        for cap in &resource.capabilities {
            html.push_str(&format!(r#"<span class="capability">{}</span>"#, cap));
        }

        html.push_str("</div><h4>Fields:</h4>");

        for field in &resource.fields {
            html.push_str(&format!(r#"<div class="field"><code>{}: {}</code></div>"#, field.name, field.ty));
        }

        html.push_str("</div>\n");
        html
    }

    /// 生成 Shared HTML
    fn generate_shared_html(&self, shared: &SharedDoc) -> String {
        let mut html = String::new();

        html.push_str(&format!(
            r#"<div class="item shared">
            <h3>shared {}</h3>
            <p>{}</p>
            <h4>Fields:</h4>
        "#,
            shared.name, shared.description
        ));

        for field in &shared.fields {
            html.push_str(&format!(r#"<div class="field"><code>{}: {}</code></div>"#, field.name, field.ty));
        }

        html.push_str("</div>\n");
        html
    }

    /// 生成 Receipt HTML
    fn generate_receipt_html(&self, receipt: &ReceiptDoc) -> String {
        let mut html = String::new();

        html.push_str(&format!(
            r#"<div class="item receipt">
            <h3>receipt {}</h3>
            <p>{}</p>
        "#,
            receipt.name, receipt.description
        ));

        if let Some(lifecycle) = &receipt.lifecycle {
            html.push_str(r#"<div class="lifecycle"><strong>Lifecycle:</strong>"#);
            for state in lifecycle {
                html.push_str(&format!(r#"<span class="lifecycle-state">{}</span>"#, state));
            }
            html.push_str("</div>");
        }

        html.push_str("<h4>Fields:</h4>");
        for field in &receipt.fields {
            html.push_str(&format!(r#"<div class="field"><code>{}: {}</code></div>"#, field.name, field.ty));
        }

        html.push_str("</div>\n");
        html
    }

    /// 生成 Struct HTML
    fn generate_struct_html(&self, struct_def: &StructDoc) -> String {
        let mut html = String::new();

        html.push_str(&format!(
            r#"<div class="item">
            <h3>struct {}</h3>
            <p>{}</p>
            <h4>Fields:</h4>
        "#,
            struct_def.name, struct_def.description
        ));

        for field in &struct_def.fields {
            html.push_str(&format!(r#"<div class="field"><code>{}: {}</code></div>"#, field.name, field.ty));
        }

        html.push_str("</div>\n");
        html
    }

    /// 生成 Action HTML
    fn generate_action_html(&self, action: &ActionDoc) -> String {
        let mut html = String::new();

        let effect_class = action.effect_class.to_lowercase();
        html.push_str(&format!(
            r#"<div class="item action">
            <h3>action {}</h3>
            <p>{}</p>
            <span class="effect-class effect-{}">{}</span>
        "#,
            action.name, action.description, effect_class, action.effect_class
        ));

        if let Some(hint) = &action.scheduler_hint {
            html.push_str(&format!(
                r#"<div class="scheduler-hint">
                <strong>Scheduler Hint:</strong> {} | {} cycles
            </div>"#,
                if hint.parallelizable { "Parallel" } else { "Sequential" },
                hint.estimated_cycles
            ));
        }

        html.push_str("<h4>Parameters:</h4>");
        for param in &action.parameters {
            html.push_str(&format!(r#"<div class="param"><code>{}: {}</code></div>"#, param.name, param.ty));
        }

        if let Some(ret) = &action.return_type {
            html.push_str(&format!(r#"<p><strong>Returns:</strong> <code>{}</code></p>"#, ret));
        }

        html.push_str("</div>\n");
        html
    }

    /// 生成 Lock HTML
    fn generate_lock_html(&self, lock: &LockDoc) -> String {
        let mut html = String::new();

        html.push_str(&format!(
            r#"<div class="item lock">
            <h3>lock {}</h3>
            <p>{}</p>
            <h4>Parameters:</h4>
        "#,
            lock.name, lock.description
        ));

        for param in &lock.parameters {
            html.push_str(&format!(r#"<div class="param"><code>{}: {}</code></div>"#, param.name, param.ty));
        }

        html.push_str(&format!(r#"<p><strong>Returns:</strong> <code>{}</code></p>"#, lock.return_type));
        html.push_str("</div>\n");
        html
    }

    /// 生成常量 HTML
    fn generate_constant_html(&self, constant: &ConstantDoc) -> String {
        format!(
            r#"<div class="item">
            <h3>const {}</h3>
            <p>{}</p>
            <p><code>{} {} = {}</code></p>
        </div>"#,
            constant.name, constant.description, constant.ty, constant.name, constant.value
        )
    }

    /// 生成 Markdown 文档
    fn generate_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str("# CellScript API Documentation\n\n");

        for module in &self.modules {
            md.push_str(&format!("## Module: {}\n\n", module.name));
            md.push_str(&format!("{}\n\n", module.description));

            for item in &module.items {
                md.push_str(&self.generate_item_markdown(item));
            }
        }

        md
    }

    /// 生成条目 Markdown
    fn generate_item_markdown(&self, item: &ItemDoc) -> String {
        match item {
            ItemDoc::Resource(r) => {
                let mut md = format!("### resource `{}`\n\n{}", r.name, r.description);
                md.push_str("\n\n**Capabilities:** ");
                md.push_str(&r.capabilities.join(", "));
                md.push_str("\n\n**Fields:**\n");
                for field in &r.fields {
                    md.push_str(&format!("- `{}: {}`\n", field.name, field.ty));
                }
                md.push_str("\n");
                md
            }
            ItemDoc::Action(a) => {
                let mut md = format!("### action `{}`\n\n{}", a.name, a.description);
                md.push_str(&format!("\n\n**Effect Class:** `{}`", a.effect_class));
                if let Some(hint) = &a.scheduler_hint {
                    md.push_str(&format!(
                        "\n\n**Scheduler Hint:** {} | {} cycles",
                        if hint.parallelizable { "Parallel" } else { "Sequential" },
                        hint.estimated_cycles
                    ));
                }
                md.push_str("\n\n**Parameters:**\n");
                for param in &a.parameters {
                    md.push_str(&format!("- `{}: {}`\n", param.name, param.ty));
                }
                if let Some(ret) = &a.return_type {
                    md.push_str(&format!("\n**Returns:** `{}`\n", ret));
                }
                md.push_str("\n");
                md
            }
            _ => String::new(),
        }
    }

    /// 生成 JSON 文档
    fn generate_json(&self) -> String {
        serde_json::to_string_pretty(&self.modules).unwrap_or_default()
    }
}

/// 提取模块文档
fn extract_module_doc(module: &Module) -> String {
    format!("Module {} containing {} items", module.name, module.items.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doc_generator() {
        let mut generator = DocGenerator::new(OutputFormat::Html);

        // 创建一个测试模块
        let module = Module {
            name: "test".to_string(),
            items: vec![Item::Resource(ResourceDef {
                name: "Token".to_string(),
                capabilities: vec![Capability::Store, Capability::Transfer],
                fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: crate::error::Span::default() }],
                span: crate::error::Span::default(),
            })],
            span: crate::error::Span::default(),
        };

        generator.add_module(&module);

        let html = generator.generate();
        assert!(html.contains("Token"));
        assert!(html.contains("resource"));
    }
}
