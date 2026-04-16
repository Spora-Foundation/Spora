//! Minimal CellScript documentation generator.

use crate::ast::*;
use crate::error::Result;
use crate::CompileMetadata;
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

#[derive(Debug, Clone, Serialize)]
pub struct AuditDoc {
    pub metadata_schema_version: u32,
    pub compiler_version: String,
    pub module: String,
    pub artifact_format: String,
    pub artifact_hash_blake3: Option<String>,
    pub artifact_size_bytes: Option<usize>,
    pub source_hash_blake3: Option<String>,
    pub source_content_hash_blake3: Option<String>,
    pub source_units: Vec<AuditSourceUnitDoc>,
    pub vm_abi_format: String,
    pub vm_abi_version: u16,
    pub vm_abi_embedded_in_artifact: bool,
    pub vm_abi_scope: String,
    pub ckb_runtime_required: bool,
    pub standalone_runner_compatible: bool,
    pub symbolic_cell_runtime_required: bool,
    pub ckb_runtime_features: Vec<String>,
    pub fail_closed_runtime_features: Vec<String>,
    pub verifier_obligations: Vec<AuditObligationDoc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditSourceUnitDoc {
    pub path: String,
    pub role: String,
    pub hash_blake3: String,
    pub size_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditObligationDoc {
    pub scope: String,
    pub category: String,
    pub feature: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentationBundle {
    pub modules: Vec<ModuleDoc>,
    pub audit: Option<AuditDoc>,
}

pub struct DocGenerator {
    modules: Vec<ModuleDoc>,
    audit: Option<AuditDoc>,
    format: OutputFormat,
}

impl DocGenerator {
    pub fn new(format: OutputFormat) -> Self {
        Self { modules: Vec::new(), audit: None, format }
    }

    pub fn add_module(&mut self, module: &Module) {
        let items = module.items.iter().filter_map(item_doc).collect::<Vec<_>>();
        self.modules.push(ModuleDoc { name: module.name.clone(), items });
    }

    pub fn set_compile_metadata(&mut self, metadata: &CompileMetadata) {
        self.audit = Some(AuditDoc {
            metadata_schema_version: metadata.metadata_schema_version,
            compiler_version: metadata.compiler_version.clone(),
            module: metadata.module.clone(),
            artifact_format: metadata.artifact_format.clone(),
            artifact_hash_blake3: metadata.artifact_hash_blake3.clone(),
            artifact_size_bytes: metadata.artifact_size_bytes,
            source_hash_blake3: metadata.source_hash_blake3.clone(),
            source_content_hash_blake3: metadata.source_content_hash_blake3.clone(),
            source_units: metadata
                .source_units
                .iter()
                .map(|unit| AuditSourceUnitDoc {
                    path: unit.path.clone(),
                    role: unit.role.clone(),
                    hash_blake3: unit.hash_blake3.clone(),
                    size_bytes: unit.size_bytes,
                })
                .collect(),
            vm_abi_format: metadata.runtime.vm_abi.format.clone(),
            vm_abi_version: metadata.runtime.vm_abi.version,
            vm_abi_embedded_in_artifact: metadata.runtime.vm_abi.embedded_in_artifact,
            vm_abi_scope: metadata.runtime.vm_abi.scope.clone(),
            ckb_runtime_required: metadata.runtime.ckb_runtime_required,
            standalone_runner_compatible: metadata.runtime.standalone_runner_compatible,
            symbolic_cell_runtime_required: metadata.runtime.symbolic_cell_runtime_required,
            ckb_runtime_features: metadata.runtime.ckb_runtime_features.clone(),
            fail_closed_runtime_features: metadata.runtime.fail_closed_runtime_features.clone(),
            verifier_obligations: metadata
                .runtime
                .verifier_obligations
                .iter()
                .map(|obligation| AuditObligationDoc {
                    scope: obligation.scope.clone(),
                    category: obligation.category.clone(),
                    feature: obligation.feature.clone(),
                    status: obligation.status.clone(),
                    detail: obligation.detail.clone(),
                })
                .collect(),
        });
    }

    pub fn generate(&self) -> Result<String> {
        match self.format {
            OutputFormat::Markdown => Ok(self.generate_markdown()),
            OutputFormat::Html => Ok(self.generate_html()),
            OutputFormat::Json => {
                Ok(serde_json::to_string_pretty(&DocumentationBundle { modules: self.modules.clone(), audit: self.audit.clone() })
                    .map_err(|error| {
                        crate::error::CompileError::new(format!("failed to serialize docs: {}", error), crate::error::Span::default())
                    })?)
            }
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
        if let Some(audit) = &self.audit {
            out.push_str(&audit.generate_markdown());
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
        if let Some(audit) = &self.audit {
            out.push_str(&audit.generate_html());
        }
        out.push_str("</body></html>");
        out
    }
}

impl AuditDoc {
    fn generate_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("## Lowering Audit Report\n\n");
        out.push_str(&format!("- Metadata schema version: `{}`\n", self.metadata_schema_version));
        out.push_str(&format!("- Compiler version: `{}`\n", self.compiler_version));
        out.push_str(&format!("- Module: `{}`\n", self.module));
        out.push_str(&format!("- Artifact format: `{}`\n", self.artifact_format));
        if let Some(hash) = &self.artifact_hash_blake3 {
            out.push_str(&format!("- Artifact hash (BLAKE3): `{}`\n", hash));
        }
        if let Some(size) = self.artifact_size_bytes {
            out.push_str(&format!("- Artifact size: `{}` bytes\n", size));
        }
        if let Some(hash) = &self.source_hash_blake3 {
            out.push_str(&format!("- Source set hash (BLAKE3): `{}`\n", hash));
        }
        if let Some(hash) = &self.source_content_hash_blake3 {
            out.push_str(&format!("- Source content hash (BLAKE3): `{}`\n", hash));
        }
        out.push_str(&format!("- VM ABI: `{}` (`0x{:04x}`)\n", self.vm_abi_format, self.vm_abi_version));
        out.push_str(&format!("- VM ABI embedded in artifact: `{}`\n", self.vm_abi_embedded_in_artifact));
        out.push_str(&format!("- VM ABI scope: `{}`\n", self.vm_abi_scope));
        out.push_str(&format!("- CKB runtime required: `{}`\n", self.ckb_runtime_required));
        out.push_str(&format!("- Standalone runner compatible: `{}`\n", self.standalone_runner_compatible));
        out.push_str(&format!("- Symbolic Cell/runtime required: `{}`\n", self.symbolic_cell_runtime_required));
        out.push_str(&format!("- CKB runtime features: `{}`\n", comma_or_none(&self.ckb_runtime_features)));
        out.push_str(&format!("- Fail-closed runtime features: `{}`\n\n", comma_or_none(&self.fail_closed_runtime_features)));

        if !self.source_units.is_empty() {
            out.push_str("### Source Units\n\n");
            out.push_str("| Role | Path | BLAKE3 | Size |\n");
            out.push_str("|---|---|---|---|\n");
            for unit in &self.source_units {
                out.push_str(&format!(
                    "| `{}` | `{}` | `{}` | `{}` bytes |\n",
                    escape_markdown_table_cell(&unit.role),
                    escape_markdown_table_cell(&unit.path),
                    escape_markdown_table_cell(&unit.hash_blake3),
                    unit.size_bytes
                ));
            }
            out.push('\n');
        }

        out.push_str("### Verifier Obligations\n\n");
        if self.verifier_obligations.is_empty() {
            out.push_str("_No verifier obligations emitted._\n\n");
            return out;
        }

        out.push_str("| Scope | Category | Feature | Status | Detail |\n");
        out.push_str("|---|---|---|---|---|\n");
        for obligation in &self.verifier_obligations {
            out.push_str(&format!(
                "| `{}` | `{}` | `{}` | `{}` | {} |\n",
                escape_markdown_table_cell(&obligation.scope),
                escape_markdown_table_cell(&obligation.category),
                escape_markdown_table_cell(&obligation.feature),
                escape_markdown_table_cell(&obligation.status),
                escape_markdown_table_cell(&obligation.detail)
            ));
        }
        out.push('\n');
        out
    }

    fn generate_html(&self) -> String {
        let mut out = String::new();
        out.push_str("<section><h2>Lowering Audit Report</h2>");
        out.push_str("<ul>");
        out.push_str(&format!("<li>Metadata schema version: <code>{}</code></li>", self.metadata_schema_version));
        out.push_str(&format!("<li>Compiler version: <code>{}</code></li>", escape_html(&self.compiler_version)));
        out.push_str(&format!("<li>Module: <code>{}</code></li>", escape_html(&self.module)));
        out.push_str(&format!("<li>Artifact format: <code>{}</code></li>", escape_html(&self.artifact_format)));
        if let Some(hash) = &self.artifact_hash_blake3 {
            out.push_str(&format!("<li>Artifact hash (BLAKE3): <code>{}</code></li>", escape_html(hash)));
        }
        if let Some(size) = self.artifact_size_bytes {
            out.push_str(&format!("<li>Artifact size: <code>{}</code> bytes</li>", size));
        }
        if let Some(hash) = &self.source_hash_blake3 {
            out.push_str(&format!("<li>Source set hash (BLAKE3): <code>{}</code></li>", escape_html(hash)));
        }
        if let Some(hash) = &self.source_content_hash_blake3 {
            out.push_str(&format!("<li>Source content hash (BLAKE3): <code>{}</code></li>", escape_html(hash)));
        }
        out.push_str(&format!(
            "<li>VM ABI: <code>{}</code> (<code>0x{:04x}</code>)</li>",
            escape_html(&self.vm_abi_format),
            self.vm_abi_version
        ));
        out.push_str(&format!("<li>VM ABI embedded in artifact: <code>{}</code></li>", self.vm_abi_embedded_in_artifact));
        out.push_str(&format!("<li>VM ABI scope: <code>{}</code></li>", escape_html(&self.vm_abi_scope)));
        out.push_str(&format!("<li>CKB runtime required: <code>{}</code></li>", self.ckb_runtime_required));
        out.push_str(&format!("<li>Standalone runner compatible: <code>{}</code></li>", self.standalone_runner_compatible));
        out.push_str(&format!("<li>Symbolic Cell/runtime required: <code>{}</code></li>", self.symbolic_cell_runtime_required));
        out.push_str(&format!(
            "<li>CKB runtime features: <code>{}</code></li>",
            escape_html(&comma_or_none(&self.ckb_runtime_features))
        ));
        out.push_str(&format!(
            "<li>Fail-closed runtime features: <code>{}</code></li>",
            escape_html(&comma_or_none(&self.fail_closed_runtime_features))
        ));
        out.push_str("</ul>");
        if !self.source_units.is_empty() {
            out.push_str("<h3>Source Units</h3>");
            out.push_str("<table><thead><tr><th>Role</th><th>Path</th><th>BLAKE3</th><th>Size</th></tr></thead><tbody>");
            for unit in &self.source_units {
                out.push_str(&format!(
                    "<tr><td><code>{}</code></td><td><code>{}</code></td><td><code>{}</code></td><td><code>{}</code> bytes</td></tr>",
                    escape_html(&unit.role),
                    escape_html(&unit.path),
                    escape_html(&unit.hash_blake3),
                    unit.size_bytes
                ));
            }
            out.push_str("</tbody></table>");
        }
        out.push_str("<h3>Verifier Obligations</h3>");
        if self.verifier_obligations.is_empty() {
            out.push_str("<p><em>No verifier obligations emitted.</em></p></section>");
            return out;
        }
        out.push_str(
            "<table><thead><tr><th>Scope</th><th>Category</th><th>Feature</th><th>Status</th><th>Detail</th></tr></thead><tbody>",
        );
        for obligation in &self.verifier_obligations {
            out.push_str(&format!(
                "<tr><td><code>{}</code></td><td><code>{}</code></td><td><code>{}</code></td><td><code>{}</code></td><td>{}</td></tr>",
                escape_html(&obligation.scope),
                escape_html(&obligation.category),
                escape_html(&obligation.feature),
                escape_html(&obligation.status),
                escape_html(&obligation.detail)
            ));
        }
        out.push_str("</tbody></table></section>");
        out
    }
}

fn comma_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn escape_markdown_table_cell(input: &str) -> String {
    input.replace('|', "\\|").replace('\n', " ")
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
            if let Some(output) = &receipt.claim_output {
                summary.push_str(&format!("Claim output: {}. ", format_type(output)));
            }
            if let Some(lifecycle) = &receipt.lifecycle {
                summary.push_str(&format!("Lifecycle: {}. ", lifecycle.states.join(" -> ")));
                let transitions =
                    lifecycle.states.windows(2).map(|window| format!("{} -> {}", window[0], window[1])).collect::<Vec<_>>();
                if !transitions.is_empty() {
                    summary.push_str(&format!("Transitions: {}. ", transitions.join(", ")));
                }
            }
            summary.push_str(&format!("Fields: {}", format_fields(&receipt.fields)));
            Some(ItemDoc {
                kind: "receipt".to_string(),
                name: receipt.name.clone(),
                signature: format!(
                    "receipt {}{}{}",
                    receipt.name,
                    receipt.claim_output.as_ref().map(|ty| format!(" -> {}", format_type(ty))).unwrap_or_default(),
                    format_capability_clause(&receipt.capabilities)
                ),
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
            signature: function_signature(function),
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

fn function_signature(function: &FnDef) -> String {
    let params = function.params.iter().map(format_param).collect::<Vec<_>>().join(", ");
    let mut signature = format!("fn {}({})", function.name, params);
    if let Some(return_type) = &function.return_type {
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
        Type::Unit => "()".to_string(),
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
