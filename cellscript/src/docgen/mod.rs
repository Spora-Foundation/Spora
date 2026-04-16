//! Minimal CellScript documentation generator.

use crate::ast::*;
use crate::error::Result;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Html,
    Markdown,
    Json,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::Html
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleDoc {
    pub name: String,
    pub items: Vec<ItemDoc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemDoc {
    pub kind: String,
    pub name: String,
    pub signature: String,
    pub summary: String,
}

pub struct DocGenerator {
    modules: Vec<ModuleDoc>,
    format: OutputFormat,
}

impl DocGenerator {
    pub fn new(format: OutputFormat) -> Self {
        Self { modules: Vec::new(), format }
    }

    pub fn add_module(&mut self, module: &Module) {
        let items = module.items.iter().filter_map(item_doc).collect::<Vec<_>>();
        self.modules.push(ModuleDoc { name: module.name.clone(), items });
    }

    pub fn generate(&self) -> Result<String> {
        match self.format {
            OutputFormat::Markdown => Ok(self.generate_markdown()),
            OutputFormat::Html => Ok(self.generate_html()),
            OutputFormat::Json => Ok(serde_json::to_string_pretty(&self.modules).map_err(|error| {
                crate::error::CompileError::new(format!("failed to serialize docs: {}", error), crate::error::Span::default())
            })?),
        }
    }

    fn generate_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# CellScript API Documentation\n\n");
        for module in &self.modules {
            out.push_str(&format!("## Module `{}`\n\n", module.name));
            if module.items.is_empty() {
                out.push_str("_No documentable items._\n\n");
                continue;
            }
            for item in &module.items {
                out.push_str(&format!("### {} `{}`\n\n", item.kind, item.name));
                out.push_str("```cellscript\n");
                out.push_str(&item.signature);
                out.push_str("\n```\n\n");
                if !item.summary.is_empty() {
                    out.push_str(&item.summary);
                    out.push_str("\n\n");
                }
            }
        }
        out
    }

    fn generate_html(&self) -> String {
        let mut out = String::new();
        out.push_str("<!doctype html><html><head><meta charset=\"utf-8\"><title>CellScript API Documentation</title>");
        out.push_str(
            "<style>body{font-family:ui-sans-serif,system-ui,sans-serif;max-width:960px;margin:0 auto;padding:32px;line-height:1.6;color:#1f2937}pre{background:#f3f4f6;padding:16px;border-radius:8px;overflow:auto}section{margin-bottom:32px}.kind{display:inline-block;padding:2px 8px;border-radius:999px;background:#e5e7eb;font-size:12px;color:#374151}</style>",
        );
        out.push_str("</head><body><h1>CellScript API Documentation</h1>");
        for module in &self.modules {
            out.push_str(&format!("<section><h2>Module <code>{}</code></h2>", module.name));
            if module.items.is_empty() {
                out.push_str("<p><em>No documentable items.</em></p></section>");
                continue;
            }
            for item in &module.items {
                out.push_str(&format!(
                    "<article><p class=\"kind\">{}</p><h3><code>{}</code></h3><pre>{}</pre>",
                    escape_html(&item.kind),
                    escape_html(&item.name),
                    escape_html(&item.signature)
                ));
                if !item.summary.is_empty() {
                    out.push_str(&format!("<p>{}</p>", escape_html(&item.summary)));
                }
                out.push_str("</article>");
            }
            out.push_str("</section>");
        }
        out.push_str("</body></html>");
        out
    }
}

fn item_doc(item: &Item) -> Option<ItemDoc> {
    match item {
        Item::Use(_) => None,
        Item::Resource(resource) => Some(ItemDoc {
            kind: "resource".to_string(),
            name: resource.name.clone(),
            signature: format!("resource {}{}", resource.name, format_capability_clause(&resource.capabilities)),
            summary: format!("Fields: {}", format_fields(&resource.fields)),
        }),
        Item::Shared(shared) => Some(ItemDoc {
            kind: "shared".to_string(),
            name: shared.name.clone(),
            signature: format!("shared {}{}", shared.name, format_capability_clause(&shared.capabilities)),
            summary: format!("Fields: {}", format_fields(&shared.fields)),
        }),
        Item::Receipt(receipt) => {
            let mut summary = String::new();
            if let Some(lifecycle) = &receipt.lifecycle {
                summary.push_str(&format!("Lifecycle: {}. ", lifecycle.states.join(" -> ")));
            }
            summary.push_str(&format!("Fields: {}", format_fields(&receipt.fields)));
            Some(ItemDoc {
                kind: "receipt".to_string(),
                name: receipt.name.clone(),
                signature: format!("receipt {}{}", receipt.name, format_capability_clause(&receipt.capabilities)),
                summary,
            })
        }
        Item::Struct(struct_def) => Some(ItemDoc {
            kind: "struct".to_string(),
            name: struct_def.name.clone(),
            signature: format!("struct {}", struct_def.name),
            summary: format!("Fields: {}", format_fields(&struct_def.fields)),
        }),
        Item::Const(constant) => Some(ItemDoc {
            kind: "const".to_string(),
            name: constant.name.clone(),
            signature: format!("const {}: {}", constant.name, format_type(&constant.ty)),
            summary: String::new(),
        }),
        Item::Enum(enum_def) => Some(ItemDoc {
            kind: "enum".to_string(),
            name: enum_def.name.clone(),
            signature: format!(
                "enum {} {{ {} }}",
                enum_def.name,
                enum_def
                    .variants
                    .iter()
                    .map(|variant| {
                        if variant.fields.is_empty() {
                            variant.name.clone()
                        } else {
                            format!("{}({})", variant.name, variant.fields.iter().map(format_type).collect::<Vec<_>>().join(", "))
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            summary: String::new(),
        }),
        Item::Action(action) => Some(ItemDoc {
            kind: "action".to_string(),
            name: action.name.clone(),
            signature: action_signature("action", action),
            summary: action.doc_comment.clone().unwrap_or_else(|| format!("Effect: {}.", format_effect(action.effect))),
        }),
        Item::Function(function) => Some(ItemDoc {
            kind: "fn".to_string(),
            name: function.name.clone(),
            signature: action_signature("fn", function),
            summary: function.doc_comment.clone().unwrap_or_else(|| "Pure helper function.".to_string()),
        }),
        Item::Lock(lock) => Some(ItemDoc {
            kind: "lock".to_string(),
            name: lock.name.clone(),
            signature: format!(
                "lock {}({}) -> {}",
                lock.name,
                lock.params.iter().map(format_param).collect::<Vec<_>>().join(", "),
                format_type(&lock.return_type)
            ),
            summary: "Lock predicate; current compiler treats lock bodies as explicit validation logic.".to_string(),
        }),
    }
}

fn action_signature(keyword: &str, action: &ActionDef) -> String {
    let params = action.params.iter().map(format_param).collect::<Vec<_>>().join(", ");
    let mut signature = format!("{} {}({})", keyword, action.name, params);
    if let Some(return_type) = &action.return_type {
        signature.push_str(&format!(" -> {}", format_type(return_type)));
    }
    signature
}

fn format_fields(fields: &[Field]) -> String {
    if fields.is_empty() {
        return "none".to_string();
    }
    fields.iter().map(|field| format!("{}: {}", field.name, format_type(&field.ty))).collect::<Vec<_>>().join(", ")
}

fn format_capability_clause(capabilities: &[Capability]) -> String {
    if capabilities.is_empty() {
        String::new()
    } else {
        format!(" has {}", capabilities.iter().map(format_capability).collect::<Vec<_>>().join(", "))
    }
}

fn format_capability(capability: &Capability) -> &'static str {
    match capability {
        Capability::Store => "store",
        Capability::Transfer => "transfer",
        Capability::Destroy => "destroy",
    }
}

fn format_effect(effect: EffectClass) -> &'static str {
    match effect {
        EffectClass::Pure => "pure",
        EffectClass::ReadOnly => "readonly",
        EffectClass::Mutating => "mutating",
        EffectClass::Creating => "creating",
        EffectClass::Destroying => "destroying",
    }
}

fn format_param(param: &Param) -> String {
    let mut rendered = String::new();
    if param.is_mut {
        rendered.push_str("mut ");
    }
    if param.is_ref {
        rendered.push('&');
    }
    rendered.push_str(&param.name);
    rendered.push_str(": ");
    rendered.push_str(&format_type(&param.ty));
    rendered
}

fn format_type(ty: &Type) -> String {
    match ty {
        Type::U8 => "u8".to_string(),
        Type::U16 => "u16".to_string(),
        Type::U32 => "u32".to_string(),
        Type::U64 => "u64".to_string(),
        Type::U128 => "u128".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Address => "Address".to_string(),
        Type::Hash => "Hash".to_string(),
        Type::Array(inner, length) => format!("[{}; {}]", format_type(inner), length),
        Type::Tuple(items) => format!("({})", items.iter().map(format_type).collect::<Vec<_>>().join(", ")),
        Type::Named(name) => name.clone(),
        Type::Ref(inner) => format!("&{}", format_type(inner)),
        Type::MutRef(inner) => format!("&mut {}", format_type(inner)),
    }
}

fn escape_html(input: &str) -> String {
    input.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser};

    #[test]
    fn docgen_emits_markdown_for_action() {
        let source = r#"
module demo

/// adds two numbers
action add(x: u64, y: u64) -> u64 {
    return x + y
}
"#;
        let tokens = lexer::lex(source).unwrap();
        let module = parser::parse(&tokens).unwrap();

        let mut generator = DocGenerator::new(OutputFormat::Markdown);
        generator.add_module(&module);
        let docs = generator.generate().unwrap();

        assert!(docs.contains("## Module `demo`"));
        assert!(docs.contains("### action `add`"));
        assert!(docs.contains("action add(x: u64, y: u64) -> u64"));
    }
}
