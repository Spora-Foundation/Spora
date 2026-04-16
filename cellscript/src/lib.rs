//! CellScript - Spora 区块链的领域特定语言编译器
//!
//! 当前后端可输出 RISC-V 汇编或 ELF 产物。

pub mod ast;
pub mod cli;
pub mod codegen;
pub mod docgen;
pub mod error;
pub mod fmt;
pub mod ir;
pub mod lexer;
pub mod lsp;
pub mod package;
pub mod parser;
pub mod repl;
pub mod resolve;
pub mod stdlib;
pub mod types;
pub mod wasm;

use camino::{Utf8Path, Utf8PathBuf};
use error::{CompileError, Result};
use resolve::ModuleResolver;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

/// 编译选项
#[derive(Debug, Clone)]
pub struct CompileOptions {
    /// 优化级别 (0-3)
    pub opt_level: u8,
    /// 输出文件路径
    pub output: Option<String>,
    /// 是否生成调试信息
    pub debug: bool,
    /// 目标产物
    pub target: Option<String>,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self { opt_level: 0, output: None, debug: false, target: None }
    }
}

const DEFAULT_TARGET: &str = "riscv64-asm";

/// 编译产物格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactFormat {
    /// RISC-V 汇编文本
    RiscvAssembly,
    /// RISC-V ELF 可执行文件
    RiscvElf,
}

impl ArtifactFormat {
    pub fn from_target(target: &str) -> Result<Self> {
        match target {
            "asm" | "riscv64" | "riscv64-asm" => Ok(Self::RiscvAssembly),
            "elf" | "riscv64-elf" => Ok(Self::RiscvElf),
            other => Err(CompileError::new(
                format!("unsupported target '{}'; supported targets: asm, riscv64-asm, riscv64, riscv64-elf", other),
                error::Span::default(),
            )),
        }
    }

    pub fn file_extension(self) -> &'static str {
        match self {
            Self::RiscvAssembly => "s",
            Self::RiscvElf => "elf",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::RiscvAssembly => "RISC-V assembly",
            Self::RiscvElf => "RISC-V ELF",
        }
    }
}

/// 编译结果
#[derive(Debug, Clone)]
pub struct CompileResult {
    /// 生成的产物字节
    pub artifact_bytes: Vec<u8>,
    /// 产物格式
    pub artifact_format: ArtifactFormat,
    /// 产物哈希
    pub artifact_hash: [u8; 32],
    /// 可供调度器/工具消费的编译元数据
    pub metadata: CompileMetadata,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompileMetadata {
    pub module: String,
    pub artifact_format: String,
    pub lowering: LoweringMetadata,
    pub runtime: RuntimeMetadata,
    pub types: Vec<TypeMetadata>,
    pub actions: Vec<ActionMetadata>,
    pub locks: Vec<LockMetadata>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoweringMetadata {
    pub protocol_semantics: String,
    pub assembly_path: String,
    pub elf_path: String,
    pub semantics_preserving_claim: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeMetadata {
    pub vm_target: String,
    pub vm_version: String,
    pub syscall_abi: String,
    pub pure_elf_runner: String,
    pub ckb_runtime_required: bool,
    pub ckb_runtime_features: Vec<String>,
    pub standalone_runner_compatible: bool,
    pub symbolic_cell_runtime_required: bool,
    pub unsupported_elf_features: Vec<String>,
    pub ckb_runtime_accesses: Vec<CkbRuntimeAccessMetadata>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CkbRuntimeAccessMetadata {
    pub operation: String,
    pub syscall: String,
    pub source: String,
    pub index: usize,
    pub binding: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypeMetadata {
    pub name: String,
    pub kind: String,
    pub encoded_size: Option<usize>,
    pub fields: Vec<FieldMetadata>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldMetadata {
    pub name: String,
    pub ty: String,
    pub offset: usize,
    pub encoded_size: Option<usize>,
    pub fixed_width: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionMetadata {
    pub name: String,
    pub params: Vec<ParamMetadata>,
    pub effect_class: String,
    pub parallelizable: bool,
    pub touches_shared: Vec<String>,
    pub estimated_cycles: u64,
    pub scheduler_witness_borsh_hex: String,
    pub consume_set: Vec<CellPatternMetadata>,
    pub read_refs: Vec<CellPatternMetadata>,
    pub create_set: Vec<CreatePatternMetadata>,
    pub ckb_runtime_accesses: Vec<CkbRuntimeAccessMetadata>,
    pub ckb_runtime_features: Vec<String>,
    pub symbolic_runtime_features: Vec<String>,
    pub elf_compatible: bool,
    pub standalone_runner_compatible: bool,
    pub block_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct LockMetadata {
    pub name: String,
    pub params: Vec<ParamMetadata>,
    pub consume_set: Vec<CellPatternMetadata>,
    pub read_refs: Vec<CellPatternMetadata>,
    pub create_set: Vec<CreatePatternMetadata>,
    pub ckb_runtime_accesses: Vec<CkbRuntimeAccessMetadata>,
    pub ckb_runtime_features: Vec<String>,
    pub symbolic_runtime_features: Vec<String>,
    pub elf_compatible: bool,
    pub standalone_runner_compatible: bool,
    pub block_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamMetadata {
    pub name: String,
    pub ty: String,
    pub is_mut: bool,
    pub is_ref: bool,
    pub schema_pointer_abi: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CellPatternMetadata {
    pub type_hash: Option<String>,
    pub binding: String,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatePatternMetadata {
    pub ty: String,
    pub binding: String,
    pub fields: Vec<String>,
    pub has_lock: bool,
}

#[derive(Debug, Clone)]
struct MetadataFieldLayout {
    ty: ir::IrType,
    fixed_size: Option<usize>,
}

type MetadataTypeLayouts = HashMap<String, HashMap<String, MetadataFieldLayout>>;

#[derive(Debug, Clone)]
pub struct LoadedModule {
    pub path: Utf8PathBuf,
    pub source: String,
    pub ast: ast::Module,
}

impl CompileResult {
    /// 默认输出路径
    pub fn default_output_path(&self, input_path: &Utf8Path) -> Utf8PathBuf {
        input_path.with_extension(self.artifact_format.file_extension())
    }

    /// 将产物写入文件
    pub fn write_to_path(&self, output_path: &Utf8Path) -> Result<()> {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                CompileError::new(format!("failed to create output directory '{}': {}", parent, e), error::Span::default())
            })?;
        }

        std::fs::write(output_path, &self.artifact_bytes)
            .map_err(|e| CompileError::new(format!("failed to write output '{}': {}", output_path, e), error::Span::default()))
    }

    pub fn default_metadata_path(&self, artifact_path: &Utf8Path) -> Utf8PathBuf {
        metadata_output_path_from_artifact(artifact_path)
    }

    pub fn write_metadata_to_path(&self, output_path: &Utf8Path) -> Result<()> {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                CompileError::new(format!("failed to create metadata directory '{}': {}", parent, e), error::Span::default())
            })?;
        }

        let json = serde_json::to_vec_pretty(&self.metadata)
            .map_err(|e| CompileError::new(format!("failed to serialize metadata: {}", e), error::Span::default()))?;
        std::fs::write(output_path, json)
            .map_err(|e| CompileError::new(format!("failed to write metadata '{}': {}", output_path, e), error::Span::default()))
    }
}

/// 解析编译输入到具体的 CellScript 源文件
pub fn resolve_input_path<P: AsRef<Utf8Path>>(input: P) -> Result<Utf8PathBuf> {
    resolve_input_file(input.as_ref())
}

/// 根据原始输入推导默认输出路径
pub fn default_output_path_for_input<P: AsRef<Utf8Path>>(
    input: P,
    resolved_input: &Utf8Path,
    artifact_format: ArtifactFormat,
) -> Result<Utf8PathBuf> {
    default_output_path_from_input(input.as_ref(), resolved_input, artifact_format)
}

pub fn default_metadata_path_for_artifact<P: AsRef<Utf8Path>>(artifact_path: P) -> Utf8PathBuf {
    metadata_output_path_from_artifact(artifact_path.as_ref())
}

pub fn load_modules_for_input<P: AsRef<Utf8Path>>(input: P) -> Result<Vec<LoadedModule>> {
    let resolved = resolve_input_path(input.as_ref())?;
    let mut files = if let Some(package_root) = find_package_root(&resolved)? {
        collect_package_cell_files(&package_root)?
    } else {
        vec![resolved.clone()]
    };

    if !files.contains(&resolved) {
        files.push(resolved);
        files.sort();
    }

    files
        .into_iter()
        .map(|path| {
            let source = std::fs::read_to_string(&path)
                .map_err(|e| CompileError::new(format!("failed to read module '{}': {}", path, e), error::Span::default()))?;
            let tokens = lexer::lex(&source).map_err(|e| e.with_file(path.clone()))?;
            let ast = parser::parse(&tokens).map_err(|e| e.with_file(path.clone()))?;
            Ok(LoadedModule { path, source, ast })
        })
        .collect()
}

/// 编译 CellScript 源代码
pub fn compile(source: &str, options: CompileOptions) -> Result<CompileResult> {
    // 1. 词法分析
    let tokens = lexer::lex(source)?;

    // 2. 解析
    let ast = parser::parse(&tokens)?;

    compile_ast(&ast, &options, None)
}

/// 只生成编译元数据，不生成 asm/elf artifact。
pub fn compile_metadata(source: &str, target: Option<String>) -> Result<CompileMetadata> {
    let tokens = lexer::lex(source)?;
    let ast = parser::parse(&tokens)?;
    let artifact_format = ArtifactFormat::from_target(target.as_deref().unwrap_or(DEFAULT_TARGET))?;
    types::check(&ast)?;
    let ir = ir::generate(&ast)?;
    Ok(compile_metadata_from_ir(&ir, artifact_format))
}

fn compile_ast(ast: &ast::Module, options: &CompileOptions, resolver: Option<(&ModuleResolver, &str)>) -> Result<CompileResult> {
    compile_ast_with_build(ast, options, resolver, None)
}

fn compile_ast_with_build(
    ast: &ast::Module,
    options: &CompileOptions,
    resolver: Option<(&ModuleResolver, &str)>,
    build: Option<&CellBuildConfig>,
) -> Result<CompileResult> {
    let artifact_format = ArtifactFormat::from_target(resolve_target(options, build))?;

    // 3. 类型检查
    if let Some((resolver, module_name)) = resolver {
        types::check_with_resolver(ast, resolver, module_name)?;
    } else {
        types::check(ast)?;
    }

    // 4. 生成 IR
    let ir = if let Some((resolver, module_name)) = resolver {
        ir::generate_with_resolver(ast, resolver, module_name)?
    } else {
        ir::generate(ast)?
    };

    // 5. 代码生成
    let codegen_options = codegen::CodegenOptions { opt_level: options.opt_level, debug: options.debug };
    let artifact_bytes = codegen::generate(&ir, &codegen_options, artifact_format)?;
    if artifact_bytes.is_empty() {
        return Err(CompileError::new("backend produced an empty artifact", error::Span::default()));
    }

    let artifact_hash = *blake3::hash(&artifact_bytes).as_bytes();

    let metadata = compile_metadata_from_ir(&ir, artifact_format);

    Ok(CompileResult { artifact_bytes, artifact_format, artifact_hash, metadata })
}

/// 从文件、包目录或 Cell.toml 编译
pub fn compile_path<P: AsRef<Utf8Path>>(path: P, options: CompileOptions) -> Result<CompileResult> {
    let resolved = resolve_input_path(path)?;
    compile_file(&resolved, options)
}

/// 从文件编译
pub fn compile_file<P: AsRef<Utf8Path>>(path: P, options: CompileOptions) -> Result<CompileResult> {
    let path = path.as_ref();
    let source =
        std::fs::read_to_string(path).map_err(|e| CompileError::new(format!("failed to read file: {}", e), error::Span::default()))?;
    let tokens = lexer::lex(&source)?;
    let ast = parser::parse(&tokens)?;
    let resolver = build_module_resolver(path, &ast)?;
    let manifest = find_package_root(path)?.map(|root| load_manifest(&root)).transpose()?;
    compile_ast_with_build(&ast, &options, Some((&resolver, &ast.name)), manifest.as_ref().map(|manifest| &manifest.build))
}

fn build_module_resolver(path: &Utf8Path, current_module: &ast::Module) -> Result<ModuleResolver> {
    let mut resolver = ModuleResolver::new();
    resolver.register_module(current_module.clone())?;

    let current_path = canonical_utf8_path(path)?;
    if let Some(package_root) = find_package_root(path)? {
        let mut visited_roots = HashSet::new();
        let mut loading_roots = Vec::new();
        load_package_modules(&mut resolver, &package_root, &current_path, &mut visited_roots, &mut loading_roots)?;
    } else if let Some(parent) = path.parent() {
        for candidate in collect_cell_files(parent)? {
            if candidate == current_path {
                continue;
            }
            register_module_file(&mut resolver, &candidate)?;
        }
    }

    Ok(resolver)
}

#[derive(Debug, Default, Deserialize)]
struct CellManifest {
    #[serde(default)]
    package: Option<CellManifestPackage>,
    #[serde(default)]
    dependencies: HashMap<String, CellDependency>,
    #[serde(default)]
    build: CellBuildConfig,
}

#[derive(Debug, Default, Deserialize)]
struct CellManifestPackage {
    #[serde(default)]
    entry: Option<String>,
    #[serde(default)]
    source_roots: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct CellBuildConfig {
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    out_dir: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CellDependency {
    Simple(String),
    Detailed(CellDependencyDetail),
}

#[derive(Debug, Default, Deserialize)]
struct CellDependencyDetail {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    git: Option<String>,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    rev: Option<String>,
}

fn find_package_root(path: &Utf8Path) -> Result<Option<Utf8PathBuf>> {
    let mut current = path.parent();
    while let Some(dir) = current {
        let manifest = dir.join("Cell.toml");
        if manifest.exists() {
            return Ok(Some(dir.to_path_buf()));
        }
        current = dir.parent();
    }
    Ok(None)
}

fn load_package_modules(
    resolver: &mut ModuleResolver,
    package_root: &Utf8Path,
    current_path: &Utf8Path,
    visited_roots: &mut HashSet<Utf8PathBuf>,
    loading_roots: &mut Vec<Utf8PathBuf>,
) -> Result<()> {
    let package_root = canonical_utf8_path(package_root)?;
    if visited_roots.contains(&package_root) {
        return Ok(());
    }
    if let Some(index) = loading_roots.iter().position(|root| root == &package_root) {
        let mut cycle = loading_roots[index..].to_vec();
        cycle.push(package_root.clone());
        let cycle = cycle.into_iter().map(|path| path.to_string()).collect::<Vec<_>>().join(" -> ");
        return Err(CompileError::new(format!("path dependency cycle detected: {}", cycle), error::Span::default()));
    }
    loading_roots.push(package_root.clone());

    for candidate in collect_package_cell_files(&package_root)? {
        if candidate == current_path {
            continue;
        }
        register_module_file(resolver, &candidate)?;
    }

    for dep_root in local_dependency_roots(&package_root)? {
        load_package_modules(resolver, &dep_root, current_path, visited_roots, loading_roots)?;
    }

    loading_roots.pop();
    visited_roots.insert(package_root);
    Ok(())
}

fn local_dependency_roots(package_root: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let manifest = load_manifest(package_root)?;

    let mut roots = Vec::new();
    for (name, dependency) in manifest.dependencies {
        match dependency {
            CellDependency::Simple(version) => {
                return Err(CompileError::new(
                    format!(
                        "dependency '{}' uses version requirement '{}' but only local path dependencies are supported",
                        name, version
                    ),
                    error::Span::default(),
                ));
            }
            CellDependency::Detailed(detail) => {
                let Some(path) = detail.path.as_deref() else {
                    return Err(CompileError::new(
                        format!(
                            "dependency '{}' does not specify a local path; only path dependencies are supported{}",
                            name,
                            dependency_hint(&detail)
                        ),
                        error::Span::default(),
                    ));
                };

                let dep_root = package_root.join(path);
                let dep_manifest = dep_root.join("Cell.toml");
                if !dep_manifest.exists() {
                    return Err(CompileError::new(
                        format!("dependency '{}' expected manifest at '{}'", name, dep_manifest),
                        error::Span::default(),
                    ));
                }

                roots.push(canonical_utf8_path(&dep_root)?);
            }
        }
    }

    Ok(roots)
}

fn resolve_input_file(input: &Utf8Path) -> Result<Utf8PathBuf> {
    if input.is_dir() {
        return resolve_package_entry(input);
    }

    if input.file_name() == Some("Cell.toml") {
        let Some(parent) = input.parent() else {
            return Err(CompileError::new("Cell.toml must live inside a package directory", error::Span::default()));
        };
        return resolve_package_entry(parent);
    }

    if input.extension() == Some("cell") {
        if !input.exists() {
            return Err(CompileError::new(format!("input file '{}' does not exist", input), error::Span::default()));
        }
        return canonical_utf8_path(input);
    }

    Err(CompileError::new(
        format!("unsupported input '{}'; expected a .cell file, package directory, or Cell.toml", input),
        error::Span::default(),
    ))
}

fn resolve_package_entry(package_root: &Utf8Path) -> Result<Utf8PathBuf> {
    let manifest = load_manifest(package_root)?;

    let entry = manifest.package.as_ref().and_then(|package| package.entry.clone()).unwrap_or_else(default_package_entry);
    let entry_path = package_root.join(entry);
    if !entry_path.exists() {
        return Err(CompileError::new(format!("package entry '{}' does not exist", entry_path), error::Span::default()));
    }

    canonical_utf8_path(&entry_path)
}

fn default_output_path_from_input(
    input: &Utf8Path,
    resolved_input: &Utf8Path,
    artifact_format: ArtifactFormat,
) -> Result<Utf8PathBuf> {
    if input.is_dir() || input.file_name() == Some("Cell.toml") {
        let package_root = if input.is_dir() {
            canonical_utf8_path(input)?
        } else {
            let Some(parent) = input.parent() else {
                return Err(CompileError::new("Cell.toml must live inside a package directory", error::Span::default()));
            };
            canonical_utf8_path(parent)?
        };
        let stem = resolved_input.file_stem().ok_or_else(|| {
            CompileError::new(format!("cannot derive output filename from '{}'", resolved_input), error::Span::default())
        })?;
        let manifest = load_manifest(&package_root)?;
        let out_dir = manifest.build.out_dir.as_deref().unwrap_or("build");
        return Ok(package_root.join(out_dir).join(format!("{}.{}", stem, artifact_format.file_extension())));
    }

    Ok(resolved_input.with_extension(artifact_format.file_extension()))
}

fn metadata_output_path_from_artifact(artifact_path: &Utf8Path) -> Utf8PathBuf {
    let file_name = artifact_path.file_name().unwrap_or("artifact");
    let metadata_name = format!("{}.meta.json", file_name);
    artifact_path.with_file_name(metadata_name)
}

fn compile_metadata_from_ir(ir: &ir::IrModule, artifact_format: ArtifactFormat) -> CompileMetadata {
    let type_layouts = metadata_type_layouts(ir);
    let unsupported_elf_features = module_symbolic_runtime_features(ir, &type_layouts);
    let ckb_runtime_features = module_ckb_runtime_features(ir);
    let ckb_runtime_accesses = module_ckb_runtime_accesses(ir);
    let has_entry_params = module_has_entry_params(ir);
    let ckb_runtime_required = !ckb_runtime_features.is_empty();
    let standalone_runner_compatible = unsupported_elf_features.is_empty() && !ckb_runtime_required && !has_entry_params;
    CompileMetadata {
        module: ir.name.clone(),
        artifact_format: artifact_format.display_name().to_string(),
        lowering: LoweringMetadata {
            protocol_semantics: "CellScript IR records consume/read_ref/create summaries before RISC-V codegen".to_string(),
            assembly_path: "riscv64-asm preserves symbolic cell/runtime operations with CKB-style syscall ABI comments and metadata"
                .to_string(),
            elf_path: if unsupported_elf_features.is_empty() {
                "riscv64-elf is enabled for pure computation/control-flow programs, restricted fixed-width schema-parameter fields, and restricted read_ref fields that use CKB runtime syscalls"
                    .to_string()
            } else {
                "riscv64-elf is fail-closed while symbolic cell/runtime features are present".to_string()
            },
            semantics_preserving_claim:
                "Pure computation lowering is executable; stateful protocol lowering is represented in metadata and asm but is not yet a proved schema decoder/verifier"
                    .to_string(),
        },
        runtime: RuntimeMetadata {
            vm_target: "CKB-VM compatible RISC-V 64 IMC+B+MOP".to_string(),
            vm_version: "VERSION2".to_string(),
            syscall_abi: "CKB store_data ABI: A0=buffer, A1=size pointer, A2=offset, A3=index, A4=source".to_string(),
            pure_elf_runner: "cellc run --features vm-runner executes no-argument pure ELF with ckb-vm 0.24".to_string(),
            ckb_runtime_required,
            ckb_runtime_features,
            standalone_runner_compatible,
            symbolic_cell_runtime_required: !unsupported_elf_features.is_empty(),
            unsupported_elf_features,
            ckb_runtime_accesses,
        },
        types: ir
            .items
            .iter()
            .filter_map(|item| match item {
                ir::IrItem::TypeDef(type_def) => Some(type_metadata(type_def)),
                _ => None,
            })
            .collect(),
        actions: ir
            .items
            .iter()
            .filter_map(|item| match item {
                ir::IrItem::Action(action) => {
                    let param_schema_vars = schema_pointer_var_ids(&action.body, &action.params);
                    let symbolic_runtime_features =
                        body_symbolic_runtime_features(&action.body, &param_schema_vars, &type_layouts);
                    let ckb_runtime_features = body_ckb_runtime_features(&action.body);
                    let standalone_runner_compatible =
                        symbolic_runtime_features.is_empty() && ckb_runtime_features.is_empty() && action.params.is_empty();
                    Some(ActionMetadata {
                        name: action.name.clone(),
                        params: action.params.iter().map(param_metadata).collect(),
                        effect_class: format!("{:?}", action.effect_class),
                        parallelizable: action.scheduler_hints.parallelizable,
                        touches_shared: action.scheduler_hints.touches_shared.iter().map(|hash| hex_hash(hash)).collect(),
                        estimated_cycles: action.scheduler_hints.estimated_cycles,
                        scheduler_witness_borsh_hex: hex_bytes(&crate::stdlib::SchedulerMetadata::generate(
                            &format!("{:?}", action.effect_class),
                            action.scheduler_hints.parallelizable,
                            action.scheduler_hints.touches_shared.clone(),
                            action.scheduler_hints.estimated_cycles,
                        )),
                        consume_set: action.body.consume_set.iter().map(cell_pattern_metadata).collect(),
                        read_refs: action.body.read_refs.iter().map(cell_pattern_metadata).collect(),
                        create_set: action.body.create_set.iter().map(create_pattern_metadata).collect(),
                        ckb_runtime_accesses: body_ckb_runtime_accesses(&action.body),
                        ckb_runtime_features,
                        elf_compatible: symbolic_runtime_features.is_empty(),
                        standalone_runner_compatible,
                        symbolic_runtime_features,
                        block_count: action.body.blocks.len(),
                    })
                }
                _ => None,
            })
            .collect(),
        locks: ir
            .items
            .iter()
            .filter_map(|item| match item {
                ir::IrItem::Lock(lock) => {
                    let param_schema_vars = schema_pointer_var_ids(&lock.body, &lock.params);
                    let symbolic_runtime_features =
                        body_symbolic_runtime_features(&lock.body, &param_schema_vars, &type_layouts);
                    let ckb_runtime_features = body_ckb_runtime_features(&lock.body);
                    let standalone_runner_compatible =
                        symbolic_runtime_features.is_empty() && ckb_runtime_features.is_empty() && lock.params.is_empty();
                    Some(LockMetadata {
                        name: lock.name.clone(),
                        params: lock.params.iter().map(param_metadata).collect(),
                        consume_set: lock.body.consume_set.iter().map(cell_pattern_metadata).collect(),
                        read_refs: lock.body.read_refs.iter().map(cell_pattern_metadata).collect(),
                        create_set: lock.body.create_set.iter().map(create_pattern_metadata).collect(),
                        ckb_runtime_accesses: body_ckb_runtime_accesses(&lock.body),
                        ckb_runtime_features,
                        elf_compatible: symbolic_runtime_features.is_empty(),
                        standalone_runner_compatible,
                        symbolic_runtime_features,
                        block_count: lock.body.blocks.len(),
                    })
                }
                _ => None,
            })
            .collect(),
    }
}

fn module_symbolic_runtime_features(ir: &ir::IrModule, type_layouts: &MetadataTypeLayouts) -> Vec<String> {
    let mut features = BTreeSet::new();
    for item in &ir.items {
        match item {
            ir::IrItem::Action(action) => {
                let param_schema_vars = schema_pointer_var_ids(&action.body, &action.params);
                features.extend(body_symbolic_runtime_features(&action.body, &param_schema_vars, type_layouts));
            }
            ir::IrItem::Lock(lock) => {
                let param_schema_vars = schema_pointer_var_ids(&lock.body, &lock.params);
                features.extend(body_symbolic_runtime_features(&lock.body, &param_schema_vars, type_layouts));
            }
            ir::IrItem::TypeDef(_) => {}
        }
    }
    features.into_iter().collect()
}

fn module_ckb_runtime_accesses(ir: &ir::IrModule) -> Vec<CkbRuntimeAccessMetadata> {
    let mut accesses = Vec::new();
    for item in &ir.items {
        match item {
            ir::IrItem::Action(action) => accesses.extend(body_ckb_runtime_accesses(&action.body)),
            ir::IrItem::Lock(lock) => accesses.extend(body_ckb_runtime_accesses(&lock.body)),
            ir::IrItem::TypeDef(_) => {}
        }
    }
    accesses
}

fn module_ckb_runtime_features(ir: &ir::IrModule) -> Vec<String> {
    let mut features = BTreeSet::new();
    for item in &ir.items {
        match item {
            ir::IrItem::Action(action) => features.extend(body_ckb_runtime_features(&action.body)),
            ir::IrItem::Lock(lock) => features.extend(body_ckb_runtime_features(&lock.body)),
            ir::IrItem::TypeDef(_) => {}
        }
    }
    features.into_iter().collect()
}

fn module_has_entry_params(ir: &ir::IrModule) -> bool {
    ir.items.iter().any(|item| match item {
        ir::IrItem::Action(action) => !action.params.is_empty(),
        ir::IrItem::Lock(lock) => !lock.params.is_empty(),
        ir::IrItem::TypeDef(_) => false,
    })
}

fn body_symbolic_runtime_features(
    body: &ir::IrBody,
    param_schema_vars: &BTreeSet<usize>,
    type_layouts: &MetadataTypeLayouts,
) -> Vec<String> {
    let mut features = BTreeSet::new();
    if !body.consume_set.is_empty() {
        features.insert("consume-input-cell".to_string());
    }
    if !body.create_set.is_empty() {
        features.insert("verify-output-cell".to_string());
    }
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::FieldAccess { obj, field, .. } => {
                    if !is_executable_schema_field_access(obj, field, param_schema_vars, type_layouts) {
                        features.insert("schema-field-access".to_string());
                    }
                }
                ir::IrInstruction::Index { .. } => {
                    features.insert("indexed-state-access".to_string());
                }
                ir::IrInstruction::Length { operand, .. } if operand_static_length(operand).is_none() => {
                    features.insert("dynamic-length".to_string());
                }
                ir::IrInstruction::TypeHash { .. } => {
                    features.insert("type-hash".to_string());
                }
                ir::IrInstruction::CollectionNew { .. }
                | ir::IrInstruction::CollectionPush { .. }
                | ir::IrInstruction::CollectionExtend { .. } => {
                    features.insert("collection-runtime".to_string());
                }
                ir::IrInstruction::ReadRef { .. } => {}
                ir::IrInstruction::Consume { .. } => {
                    features.insert("consume-expression".to_string());
                }
                ir::IrInstruction::Create { .. } => {
                    features.insert("create-expression".to_string());
                }
                ir::IrInstruction::Transfer { .. } => {
                    features.insert("transfer-expression".to_string());
                }
                ir::IrInstruction::Destroy { .. } => {
                    features.insert("destroy-expression".to_string());
                }
                ir::IrInstruction::Claim { .. } => {
                    features.insert("claim-expression".to_string());
                }
                ir::IrInstruction::Settle { .. } => {
                    features.insert("settle-expression".to_string());
                }
                _ => {}
            }
        }
    }
    features.into_iter().collect()
}

fn body_ckb_runtime_features(body: &ir::IrBody) -> Vec<String> {
    let mut features = BTreeSet::new();
    if !body.consume_set.is_empty() {
        features.insert("consume-input-cell".to_string());
    }
    if !body.read_refs.is_empty() {
        features.insert("read-cell-dep".to_string());
    }
    if !body.create_set.is_empty() {
        features.insert("verify-output-cell".to_string());
    }
    features.into_iter().collect()
}

fn is_executable_schema_field_access(
    obj: &ir::IrOperand,
    field: &str,
    param_schema_vars: &BTreeSet<usize>,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let ir::IrOperand::Var(var) = obj else {
        return false;
    };
    if !param_schema_vars.contains(&var.id) {
        return false;
    }
    let Some(type_name) = named_type_name(&var.ty) else {
        return false;
    };
    let Some(layout) = type_layouts.get(type_name).and_then(|fields| fields.get(field)) else {
        return false;
    };
    executable_scalar_width(&layout.ty, layout.fixed_size).is_some()
}

fn executable_scalar_width(ty: &ir::IrType, fixed_size: Option<usize>) -> Option<usize> {
    match (ty, fixed_size) {
        (ir::IrType::Bool | ir::IrType::U8, Some(1)) => Some(1),
        (ir::IrType::U16, Some(2)) => Some(2),
        (ir::IrType::U32, Some(4)) => Some(4),
        (ir::IrType::U64, Some(8)) => Some(8),
        _ => None,
    }
}

fn schema_pointer_var_ids(body: &ir::IrBody, params: &[ir::IrParam]) -> BTreeSet<usize> {
    let mut vars =
        params.iter().filter(|param| named_type_name(&param.ty).is_some()).map(|param| param.binding.id).collect::<BTreeSet<_>>();

    for block in &body.blocks {
        for instruction in &block.instructions {
            if let ir::IrInstruction::ReadRef { dest, .. } = instruction {
                vars.insert(dest.id);
            }
            if let Some(var_id) = consumed_schema_var_id(instruction) {
                vars.insert(var_id);
            }
        }
    }

    vars
}

fn consumed_schema_var_id(instruction: &ir::IrInstruction) -> Option<usize> {
    let operand = match instruction {
        ir::IrInstruction::Consume { operand }
        | ir::IrInstruction::Transfer { operand, .. }
        | ir::IrInstruction::Settle { operand } => operand,
        ir::IrInstruction::Claim { receipt, .. } => receipt,
        _ => return None,
    };
    match operand {
        ir::IrOperand::Var(var) if named_type_name(&var.ty).is_some() => Some(var.id),
        _ => None,
    }
}

fn body_ckb_runtime_accesses(body: &ir::IrBody) -> Vec<CkbRuntimeAccessMetadata> {
    let mut accesses = Vec::new();
    for (index, pattern) in body.consume_set.iter().enumerate() {
        accesses.push(CkbRuntimeAccessMetadata {
            operation: "consume".to_string(),
            syscall: "LOAD_CELL".to_string(),
            source: "Input".to_string(),
            index,
            binding: pattern.binding.clone(),
        });
    }
    for (index, pattern) in body.read_refs.iter().enumerate() {
        accesses.push(CkbRuntimeAccessMetadata {
            operation: "read_ref".to_string(),
            syscall: "LOAD_CELL".to_string(),
            source: "CellDep".to_string(),
            index,
            binding: pattern.binding.clone(),
        });
    }
    for (index, pattern) in body.create_set.iter().enumerate() {
        accesses.push(CkbRuntimeAccessMetadata {
            operation: "create".to_string(),
            syscall: "LOAD_CELL".to_string(),
            source: "Output".to_string(),
            index,
            binding: pattern.binding.clone(),
        });
    }
    accesses
}

fn metadata_type_layouts(ir: &ir::IrModule) -> MetadataTypeLayouts {
    let mut layouts = HashMap::new();
    for item in &ir.items {
        let ir::IrItem::TypeDef(type_def) = item else {
            continue;
        };
        let fields = type_def
            .fields
            .iter()
            .map(|field| (field.name.clone(), MetadataFieldLayout { ty: field.ty.clone(), fixed_size: field.fixed_size }))
            .collect();
        layouts.insert(type_def.name.clone(), fields);
    }
    layouts
}

fn type_metadata(type_def: &ir::IrTypeDef) -> TypeMetadata {
    TypeMetadata {
        name: type_def.name.clone(),
        kind: format!("{:?}", type_def.kind),
        encoded_size: type_encoded_size(type_def),
        fields: type_def.fields.iter().map(field_metadata).collect(),
    }
}

fn field_metadata(field: &ir::IrField) -> FieldMetadata {
    FieldMetadata {
        name: field.name.clone(),
        ty: ir_type_to_string(&field.ty),
        offset: field.offset,
        encoded_size: field.fixed_size,
        fixed_width: field.fixed_size.is_some(),
    }
}

fn type_encoded_size(type_def: &ir::IrTypeDef) -> Option<usize> {
    type_def.fields.iter().try_fold(0usize, |acc, field| field.fixed_size.map(|size| acc + size))
}

fn ir_type_to_string(ty: &ir::IrType) -> String {
    match ty {
        ir::IrType::U8 => "u8".to_string(),
        ir::IrType::U16 => "u16".to_string(),
        ir::IrType::U32 => "u32".to_string(),
        ir::IrType::U64 => "u64".to_string(),
        ir::IrType::U128 => "u128".to_string(),
        ir::IrType::Bool => "bool".to_string(),
        ir::IrType::Address => "Address".to_string(),
        ir::IrType::Hash => "Hash".to_string(),
        ir::IrType::Array(inner, size) => format!("[{}; {}]", ir_type_to_string(inner), size),
        ir::IrType::Tuple(items) => {
            let fields = items.iter().map(ir_type_to_string).collect::<Vec<_>>().join(", ");
            format!("({})", fields)
        }
        ir::IrType::Named(name) => name.clone(),
        ir::IrType::Ref(inner) => format!("&{}", ir_type_to_string(inner)),
        ir::IrType::MutRef(inner) => format!("&mut {}", ir_type_to_string(inner)),
    }
}

fn named_type_name(ty: &ir::IrType) -> Option<&str> {
    match ty {
        ir::IrType::Named(name) => Some(name.as_str()),
        ir::IrType::Ref(inner) | ir::IrType::MutRef(inner) => named_type_name(inner),
        _ => None,
    }
}

fn operand_static_length(operand: &ir::IrOperand) -> Option<usize> {
    match operand {
        ir::IrOperand::Var(var) => type_static_length(&var.ty),
        ir::IrOperand::Const(ir::IrConst::Array(items)) => Some(items.len()),
        _ => None,
    }
}

fn type_static_length(ty: &ir::IrType) -> Option<usize> {
    match ty {
        ir::IrType::Array(_, size) => Some(*size),
        ir::IrType::Ref(inner) | ir::IrType::MutRef(inner) => type_static_length(inner),
        _ => None,
    }
}

fn param_metadata(param: &ir::IrParam) -> ParamMetadata {
    ParamMetadata {
        name: param.name.clone(),
        ty: ir_type_to_string(&param.ty),
        is_mut: param.is_mut,
        is_ref: param.is_ref,
        schema_pointer_abi: named_type_name(&param.ty).is_some(),
    }
}

fn cell_pattern_metadata(pattern: &ir::CellPattern) -> CellPatternMetadata {
    CellPatternMetadata {
        type_hash: pattern.type_hash.as_ref().map(hex_hash),
        binding: pattern.binding.clone(),
        fields: pattern.fields.iter().map(|(field, _)| field.clone()).collect(),
    }
}

fn create_pattern_metadata(pattern: &ir::CreatePattern) -> CreatePatternMetadata {
    CreatePatternMetadata {
        ty: pattern.ty.clone(),
        binding: pattern.binding.clone(),
        fields: pattern.fields.iter().map(|(field, _)| field.clone()).collect(),
        has_lock: pattern.lock.is_some(),
    }
}

fn hex_hash(bytes: &[u8; 32]) -> String {
    hex_bytes(bytes)
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{:02x}", byte);
    }
    out
}

fn collect_package_cell_files(package_root: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let manifest = load_manifest(package_root)?;
    let mut roots = Vec::new();
    let mut seen_roots = HashSet::new();

    if let Some(package) = &manifest.package {
        if !package.source_roots.is_empty() {
            for source_root in &package.source_roots {
                let root = package_root.join(source_root);
                if !root.exists() {
                    return Err(CompileError::new(
                        format!("configured source root '{}' does not exist", root),
                        error::Span::default(),
                    ));
                }
                if !root.is_dir() {
                    return Err(CompileError::new(
                        format!("configured source root '{}' is not a directory", root),
                        error::Span::default(),
                    ));
                }

                let root = canonical_utf8_path(&root)?;
                if seen_roots.insert(root.clone()) {
                    roots.push(root);
                }
            }
        }
    }

    if roots.is_empty() {
        let src_root = package_root.join("src");
        if src_root.exists() && src_root.is_dir() {
            let src_root = canonical_utf8_path(&src_root)?;
            if seen_roots.insert(src_root.clone()) {
                roots.push(src_root);
            }
        }
    }

    let mut explicit_entry = None;
    if let Some(entry) = manifest.package.as_ref().and_then(|package| package.entry.clone()) {
        let entry_path = package_root.join(entry);
        if !entry_path.exists() {
            return Err(CompileError::new(format!("package entry '{}' does not exist", entry_path), error::Span::default()));
        }
        let entry_path = canonical_utf8_path(&entry_path)?;
        if let Some(entry_parent) = entry_path.parent() {
            let entry_parent = canonical_utf8_path(entry_parent)?;
            if seen_roots.insert(entry_parent.clone()) {
                roots.push(entry_parent);
            }
        }
        explicit_entry = Some(entry_path);
    }

    let mut files = Vec::new();
    let mut seen_files = HashSet::new();
    for root in roots {
        for file in collect_cell_files(&root)? {
            if seen_files.insert(file.clone()) {
                files.push(file);
            }
        }
    }

    if let Some(entry_path) = explicit_entry {
        if seen_files.insert(entry_path.clone()) {
            files.push(entry_path);
        }
    }

    files.sort();
    Ok(files)
}

fn collect_cell_files(root: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let mut files = Vec::new();
    collect_cell_files_recursive(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_cell_files_recursive(root: &Utf8Path, files: &mut Vec<Utf8PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(root)
        .map_err(|e| CompileError::new(format!("failed to read module directory '{}': {}", root, e), error::Span::default()))?;

    for entry in entries {
        let entry = entry.map_err(|e| CompileError::new(format!("failed to read directory entry: {}", e), error::Span::default()))?;
        let path = entry.path();
        let Ok(candidate) = Utf8PathBuf::from_path_buf(path) else {
            continue;
        };

        if candidate.is_dir() {
            if should_skip_cell_dir(&candidate) {
                continue;
            }
            collect_cell_files_recursive(&candidate, files)?;
            continue;
        }

        if candidate.extension() == Some("cell") {
            files.push(canonical_utf8_path(&candidate)?);
        }
    }

    Ok(())
}

fn should_skip_cell_dir(path: &Utf8Path) -> bool {
    matches!(path.file_name(), Some(".git" | ".cell" | "target"))
}

fn register_module_file(resolver: &mut ModuleResolver, path: &Utf8Path) -> Result<()> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| CompileError::new(format!("failed to read module file '{}': {}", path, e), error::Span::default()))?;
    let tokens = lexer::lex(&source).map_err(|e| e.with_file(path.to_path_buf()))?;
    let module = parser::parse(&tokens).map_err(|e| e.with_file(path.to_path_buf()))?;
    resolver.register_module(module)
}

fn canonical_utf8_path(path: &Utf8Path) -> Result<Utf8PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| CompileError::new(format!("failed to canonicalize '{}': {}", path, e), error::Span::default()))?;
    Utf8PathBuf::from_path_buf(canonical)
        .map_err(|non_utf8| CompileError::new(format!("path is not valid UTF-8: {}", non_utf8.display()), error::Span::default()))
}

fn default_package_entry() -> String {
    "src/main.cell".to_string()
}

fn load_manifest(package_root: &Utf8Path) -> Result<CellManifest> {
    let manifest_path = package_root.join("Cell.toml");
    if !manifest_path.exists() {
        return Err(CompileError::new(format!("Cell.toml not found in '{}'", package_root), error::Span::default()));
    }

    let manifest_source = std::fs::read_to_string(&manifest_path)
        .map_err(|e| CompileError::new(format!("failed to read manifest '{}': {}", manifest_path, e), error::Span::default()))?;
    toml::from_str(&manifest_source)
        .map_err(|e| CompileError::new(format!("failed to parse manifest '{}': {}", manifest_path, e), error::Span::default()))
}

fn dependency_hint(detail: &CellDependencyDetail) -> String {
    if let Some(git) = &detail.git {
        return format!(" (git dependency '{}')", git);
    }
    if let Some(version) = &detail.version {
        return format!(" (version '{}')", version);
    }
    if detail.branch.is_some() || detail.tag.is_some() || detail.rev.is_some() {
        return " (non-path source metadata present)".to_string();
    }
    String::new()
}

fn resolve_target<'a>(options: &'a CompileOptions, build: Option<&'a CellBuildConfig>) -> &'a str {
    options.target.as_deref().or_else(|| build.and_then(|build| build.target.as_deref())).unwrap_or(DEFAULT_TARGET)
}

/// 编译器版本
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 编译器名称
pub const NAME: &str = "cellc";

#[cfg(test)]
mod tests {
    use super::{
        compile, compile_file, compile_path, default_output_path_for_input, load_modules_for_input, resolve_input_path,
        ArtifactFormat, CompileOptions,
    };
    use crate::{ir, lexer, parser};
    use camino::{Utf8Path, Utf8PathBuf};
    use std::{env, process::Command};
    use tempfile::tempdir;

    const SIMPLE_PROGRAM: &str = r#"
module test

action add(x: u64, y: u64) -> u64 {
    let z = x + y
    return z
}
"#;

    const IF_PROGRAM: &str = r#"
module test

action increment_if(flag: bool, x: u64) -> u64 {
    if flag {
        let tmp = x + 1
    }
    return x
}
"#;

    const CALL_PROGRAM: &str = r#"
module test

action double(x: u64) -> u64 {
    return x + x
}

action run(y: u64) -> u64 {
    let z = double(y)
    return z
}
"#;

    const IF_EXPR_PROGRAM: &str = r#"
module test

action choose(flag: bool, x: u64) -> u64 {
    let y = if flag { x + 1 } else { x + 2 }
    return y
}
"#;

    const WHILE_PROGRAM: &str = r#"
module test

action spin(flag: bool, x: u64) -> u64 {
    while flag {
        let tmp = x + 1
    }
    return x
}
"#;

    const FOR_RANGE_PROGRAM: &str = r#"
module test

action visit(n: u64) -> u64 {
    for i in 0..n {
        let tmp = i + 1
    }
    return n
}
"#;

    const ASSIGN_PROGRAM: &str = r#"
module test

action countdown(n: u64) -> u64 {
    let mut x: u64 = n
    while x > 0 {
        x = x - 1
    }
    x += 1
    return x
}
"#;

    const STRUCT_FIELD_PROGRAM: &str = r#"
module test

struct Point {
    x: u64,
    y: u64,
}

action tweak() -> u64 {
    let mut p = Point { x: 1, y: 2 }
    let a = p.x
    p.x = a + 4
    p.x += 1
    return p.x
}
"#;

    const IF_MISMATCH_PROGRAM: &str = r#"
module test

action choose(flag: bool) -> u64 {
    let y = if flag { 1 } else { false }
    return 1
}
"#;

    const UNKNOWN_FIELD_PROGRAM: &str = r#"
module test

struct Point {
    x: u64,
}

action read(p: Point) -> u64 {
    return p.y
}
"#;

    const UNKNOWN_FUNCTION_PROGRAM: &str = r#"
module test

action run(x: u64) -> u64 {
    return missing(x)
}
"#;

    const CONSTANT_PROGRAM: &str = r#"
module test

const STEP: u64 = 3;

action bump(x: u64) -> u64 {
    return x + STEP
}
"#;

    const CAST_PROGRAM: &str = r#"
module test

action widen(x: u16) -> u64 {
    return x as u64
}
"#;

    const CREATE_PROGRAM: &str = r#"
module test

resource Token {
    amount: u64,
}

action mint(owner: Address) -> Token {
    let token = create Token {
        amount: 42
    } with_lock(owner)
    return token
}
"#;

    const CREATE_VERIFY_PROGRAM: &str = r#"
module test

resource Token {
    amount: u64,
}

action issue() -> Token {
    let token = create Token {
        amount: 42
    }
    return token
}
"#;

    const CONSUME_DESTROY_PROGRAM: &str = r#"
module test

resource Token {
    amount: u64,
}

action burn(a: Token, b: Token) {
    consume a
    destroy b
}
"#;

    const PARAM_FIELD_PROGRAM: &str = r#"
module test

struct Snapshot {
    amount: u64,
}

action inspect(snapshot: Snapshot) -> u64 {
    return snapshot.amount
}
"#;

    const PACKED_SCALAR_FIELD_PROGRAM: &str = r#"
module test

shared Flags {
    enabled: bool,
    nonce: u32,
}

action inspect() -> u32 {
    let flags = read_ref<Flags>()
    let enabled = flags.enabled
    return flags.nonce
}
"#;

    const CREATE_SCALAR_VERIFY_PROGRAM: &str = r#"
module test

resource Flags {
    enabled: bool,
    nonce: u32,
}

action issue(nonce: u32) -> Flags {
    let enabled = true
    let out = create Flags {
        enabled: enabled,
        nonce: nonce
    }
    return out
}
"#;

    const READ_REF_FIELD_PROGRAM: &str = r#"
module test

shared Config {
    threshold: u64,
}

action inspect() -> u64 {
    let cfg = read_ref<Config>()
    return cfg.threshold
}
"#;

    const CONSUME_FIELD_PROGRAM: &str = r#"
module test

resource Token {
    amount: u64,
}

action inspect(token: Token) -> u64 {
    let amount = token.amount
    consume token
    return amount
}
"#;

    const CONSUME_CREATE_CONSERVATION_PROGRAM: &str = r#"
module test

resource Token {
    amount: u64,
}

action pass(token: Token) -> Token {
    let amount = token.amount
    consume token
    let out = create Token {
        amount: amount
    }
    return out
}
"#;

    const CONSUME_CREATE_ARITHMETIC_CONSERVATION_PROGRAM: &str = r#"
module test

resource Token {
    amount: u64,
}

action withdraw(token: Token, fee: u64) -> Token {
    let amount = token.amount
    let remaining = amount - fee
    consume token
    let out = create Token {
        amount: remaining
    }
    return out
}
"#;

    const CONSUME_CREATE_CHAINED_ARITHMETIC_CONSERVATION_PROGRAM: &str = r#"
module test

resource Token {
    amount: u64,
}

action withdraw(token: Token, fee: u64, tax: u64) -> Token {
    let amount = token.amount
    let after_fee = amount - fee
    let remaining = after_fee - tax
    consume token
    let out = create Token {
        amount: remaining
    }
    return out
}
"#;

    const CONSUME_CREATE_LOCAL_CONST_CONSERVATION_PROGRAM: &str = r#"
module test

resource Token {
    amount: u64,
}

action withdraw(token: Token) -> Token {
    let amount = token.amount
    let fee = 2
    let remaining = amount - fee
    consume token
    let out = create Token {
        amount: remaining
    }
    return out
}
"#;

    const DUPLICATE_READ_REF_PROGRAM: &str = r#"
module test

shared Config {
    threshold: u64,
}

action inspect() -> u64 {
    let left = read_ref<Config>()
    let right = read_ref<Config>()
    return left.threshold + right.threshold
}
"#;

    const INDEXED_TUPLE_PROGRAM: &str = r#"
module test

action second(entries: [(Address, u64); 2]) -> u64 {
    return entries[0].1
}
"#;

    const FOREACH_ARRAY_PROGRAM: &str = r#"
module test

action sum(items: [u64; 3]) -> u64 {
    let mut total: u64 = 0
    for item in items {
        total += item
    }
    return total
}
"#;

    const LEN_METHOD_PROGRAM: &str = r#"
module test

action count(items: [u64; 3]) -> u64 {
    return items.len()
}
"#;

    const MATCH_PROGRAM: &str = r#"
module test

enum Flag {
    On,
    Off,
}

action select(flag: Flag) -> u64 {
    return match flag {
        Flag::On => 1,
        _ => 2,
    }
}
"#;

    const VEC_BUILTIN_PROGRAM: &str = r#"
module test

action pack(bytes: [u8; 3]) -> u64 {
    let mut data = Vec::new()
    data.push(1)
    data.extend_from_slice(bytes)
    return data.len()
}
"#;

    const TYPE_HASH_PROGRAM: &str = r#"
module test

struct Pool {
    amount: u64,
}

action pool_id(pool: Pool) -> Hash {
    return pool.type_hash()
}
"#;

    const ZERO_PROGRAM: &str = r#"
module test

action is_zero(target: Address) -> bool {
    return target == Address::zero()
}
"#;

    const SUMMARY_PROGRAM: &str = r#"
module test

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

    const BAD_LOCK_PROGRAM: &str = r#"
module test

lock invalid(owner: Address) -> u64 {
    return 1
}
"#;

    const TRANSFER_CLAIM_SETTLE_PROGRAM: &str = r#"
module test

resource Token has store, transfer, destroy {
    amount: u64,
}

receipt VestingReceipt {
    amount: u64,
}

action move_token(token: Token, to: Address) -> Token {
    return transfer token to to
}

action redeem(receipt: VestingReceipt) -> u64 {
    return claim receipt
}

action finalize(token: Token) -> Token {
    return settle token
}
"#;

    #[test]
    fn compile_produces_non_empty_riscv_assembly() {
        let result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();

        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly);
        assert!(!result.artifact_bytes.is_empty());

        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();
        assert!(asm.contains(".section .text"));
        assert!(asm.contains(".global add"));
    }

    #[test]
    fn compile_spills_parameters_and_returns_computed_value() {
        let result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("sd a0, 0(sp)"), "missing parameter spill for x:\n{}", asm);
        assert!(asm.contains("sd a1, 8(sp)"), "missing parameter spill for y:\n{}", asm);
        assert!(asm.contains("add t0, t0, t1"), "missing add instruction:\n{}", asm);
        assert!(asm.contains("ld a0, 16(sp)"), "missing return load for z:\n{}", asm);
    }

    #[test]
    fn compile_lowers_if_statement_into_basic_blocks() {
        let result = compile(IF_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("beqz t0, .Lblock_2"), "missing conditional branch to else block:\n{}", asm);
        assert!(asm.contains(".Lblock_1:"), "missing then block label:\n{}", asm);
        assert!(asm.contains(".Lblock_2:"), "missing else block label:\n{}", asm);
        assert!(asm.contains(".Lblock_3:"), "missing join block label:\n{}", asm);
        assert!(!asm.contains(".Lelse:"), "stale hard-coded else label leaked into assembly:\n{}", asm);
    }

    #[test]
    fn compile_emits_direct_user_function_calls() {
        let result = compile(CALL_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains(".global double"), "missing callee symbol:\n{}", asm);
        assert!(asm.contains(".global run"), "missing caller symbol:\n{}", asm);
        assert!(asm.contains("call double"), "missing direct call instruction:\n{}", asm);
        assert!(asm.contains("sd a0, 8(sp)"), "missing call result spill:\n{}", asm);
    }

    #[test]
    fn compile_lowers_if_expression_with_join_move() {
        let result = compile(IF_EXPR_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("beqz t0, .Lblock_2"), "missing conditional branch for if expression:\n{}", asm);
        assert!(asm.contains(".Lblock_3:"), "missing join block for if expression:\n{}", asm);
        assert!(asm.contains("sd t0, 24(sp)") || asm.contains("sd t0, 32(sp)"), "missing branch value move into join slot:\n{}", asm);
    }

    #[test]
    fn compile_lowers_while_statement_into_loop_cfg() {
        let result = compile(WHILE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains(".Lblock_1:"), "missing while condition block:\n{}", asm);
        assert!(asm.contains(".Lblock_2:"), "missing while body block:\n{}", asm);
        assert!(asm.contains(".Lblock_3:"), "missing while exit block:\n{}", asm);
        assert!(asm.contains("beqz t0, .Lblock_3"), "missing while false-branch jump:\n{}", asm);
        assert!(asm.contains("j .Lblock_1"), "missing while back edge:\n{}", asm);
    }

    #[test]
    fn compile_lowers_for_range_into_counted_loop_cfg() {
        let result = compile(FOR_RANGE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains(".Lblock_1:"), "missing for-loop condition block:\n{}", asm);
        assert!(asm.contains(".Lblock_2:"), "missing for-loop body block:\n{}", asm);
        assert!(asm.contains(".Lblock_3:"), "missing for-loop exit block:\n{}", asm);
        assert!(asm.contains("slt t0, t0, t1"), "missing range bound comparison:\n{}", asm);
        assert!(asm.contains("li t1, 1"), "missing range increment constant:\n{}", asm);
        assert!(asm.contains("j .Lblock_1"), "missing for-loop back edge:\n{}", asm);
    }

    #[test]
    fn compile_lowers_mutable_assignments_in_loop_bodies() {
        let result = compile(ASSIGN_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("sub t0, t0, t1"), "missing subtraction for x = x - 1:\n{}", asm);
        assert!(asm.contains("add t0, t0, t1"), "missing addition for x += 1:\n{}", asm);
        assert!(asm.contains("sd t0, 8(sp)"), "missing assignment write-back into x slot:\n{}", asm);
    }

    #[test]
    fn compile_lowers_local_struct_field_reads_and_writes() {
        let result = compile(STRUCT_FIELD_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains(".global tweak"), "missing tweak symbol:\n{}", asm);
        assert!(asm.contains("li t0, 1"), "missing initial x field constant:\n{}", asm);
        assert!(asm.contains("li t0, 2"), "missing initial y field constant:\n{}", asm);
        assert!(asm.contains("sd t0, 8(sp)"), "missing field x storage slot writes:\n{}", asm);
        assert!(!asm.contains("# field access .x"), "local struct field access fell back to placeholder path:\n{}", asm);
    }

    #[test]
    fn compile_rejects_if_expression_branch_type_mismatch() {
        let err = compile(IF_MISMATCH_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("if expression branches must have matching types"));
    }

    #[test]
    fn compile_rejects_unknown_struct_fields() {
        let err = compile(UNKNOWN_FIELD_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("unknown field 'y'"));
        assert!(err.message.contains("Point"));
    }

    #[test]
    fn compile_rejects_unknown_functions() {
        let err = compile(UNKNOWN_FUNCTION_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("unknown function 'missing'"));
    }

    #[test]
    fn compile_lowers_local_constants_into_real_operands() {
        let result = compile(CONSTANT_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("li t1, 3") || asm.contains("li t0, 3"), "missing literal load for local constant:\n{}", asm);
        assert!(asm.contains("add t0, t0, t1"), "missing arithmetic using lowered constant:\n{}", asm);
    }

    #[test]
    fn compile_lowers_numeric_cast_without_zero_placeholder() {
        let result = compile(CAST_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(!asm.contains("li t0, 0"), "cast lowering regressed to zero placeholder:\n{}", asm);
        assert!(asm.contains(".global widen"), "missing widened function symbol:\n{}", asm);
    }

    #[test]
    fn compile_preserves_create_instructions_in_assembly() {
        let result = compile(CREATE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# create Token"), "create expression vanished from assembly:\n{}", asm);
        assert!(asm.contains("#   field amount = 42"), "create fields were not preserved in assembly comments:\n{}", asm);
        assert!(asm.contains("#   with_lock <expr>"), "create lock binding vanished from assembly:\n{}", asm);
    }

    #[test]
    fn compile_emits_create_output_field_verification_for_fixed_u64_fields() {
        let result = compile(CREATE_VERIFY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: LOAD_CELL reason=create source=Output index=0"),
            "create output was not loaded from CKB Output source:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: bounds check Token.amount required=8"),
            "create output verification missed field bounds check:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: verify output field Token.amount offset=0 size=8"),
            "create output field verification was not emitted:\n{}",
            asm
        );
        assert!(asm.contains("li t1, 42"), "create output verification did not load expected constant:\n{}", asm);
        assert!(asm.contains("sub t2, t0, t1"), "create output verification did not compare actual and expected values:\n{}", asm);
    }

    #[test]
    fn compile_preserves_consume_and_destroy_instructions_in_assembly() {
        let result = compile(CONSUME_DESTROY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# consume"), "consume expression vanished from assembly:\n{}", asm);
        assert!(asm.contains("# destroy"), "destroy expression vanished from assembly:\n{}", asm);
    }

    #[test]
    fn compile_preserves_read_ref_instructions_in_assembly() {
        let result = compile(SUMMARY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# read_ref Config"), "read_ref expression vanished from assembly:\n{}", asm);
    }

    #[test]
    fn compile_lowers_read_ref_schema_field_to_ckb_runtime_assembly() {
        let result = compile(READ_REF_FIELD_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: LOAD_CELL reason=read_ref source=CellDep index=0"),
            "read_ref did not lower to CKB LOAD_CELL CellDep ABI:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: bounds check Config.threshold required=8"),
            "read_ref schema field access did not emit a loaded-byte bounds check:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: schema field Config.threshold offset=0 size=8"),
            "read_ref schema field access did not expose concrete layout:\n{}",
            asm
        );
        assert!(
            asm.contains("lbu t2, 0(t4)") && asm.contains("slli t2, t2, 56"),
            "read_ref u64 field access did not lower to an unaligned-safe byte load sequence:\n{}",
            asm
        );
    }

    #[test]
    fn compile_binds_duplicate_read_refs_by_order_not_name() {
        let result = compile(DUPLICATE_READ_REF_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: LOAD_CELL reason=read_ref source=CellDep index=0"),
            "first read_ref did not bind to CellDep index 0:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL reason=read_ref source=CellDep index=1"),
            "second read_ref did not bind to CellDep index 1:\n{}",
            asm
        );
        assert_eq!(
            asm.matches("# cellscript abi: bounds check Config.threshold required=8").count(),
            2,
            "duplicate read_refs should each have a schema bounds check:\n{}",
            asm
        );
    }

    #[test]
    fn compile_emits_ckb_style_load_cell_abi_for_cell_runtime_summary() {
        let result = compile(SUMMARY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: LOAD_CELL reason=consume source=Input index=0"),
            "consume summary did not use CKB Source::Input LOAD_CELL ABI:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL reason=read_ref source=CellDep index=0"),
            "read_ref summary did not use CKB Source::CellDep LOAD_CELL ABI:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL reason=create source=Output index=0"),
            "create summary did not use CKB Source::Output LOAD_CELL ABI:\n{}",
            asm
        );
        assert!(asm.contains("addi a0, sp,"), "store_data buffer address was not prepared:\n{}", asm);
        assert!(asm.contains("addi a1, sp,"), "store_data size pointer was not prepared:\n{}", asm);
        assert!(asm.contains("li a2, 0"), "store_data offset was not prepared:\n{}", asm);
        assert!(asm.contains("li a3, 0"), "LOAD_CELL index register was not prepared:\n{}", asm);
        assert!(asm.contains("li a4, 1"), "Input source register was not prepared:\n{}", asm);
        assert!(asm.contains("li a4, 2"), "Output source register was not prepared:\n{}", asm);
        assert!(asm.contains("li a4, 3"), "CellDep source register was not prepared:\n{}", asm);
        assert!(asm.contains("li a7, 2071"), "LOAD_CELL syscall number was not emitted:\n{}", asm);
        assert!(!asm.contains("li a7, 2073"), "summary consume regressed to LOAD_INPUT with stale argument order:\n{}", asm);
    }

    #[test]
    fn compile_preserves_schema_backed_parameter_field_access_in_assembly() {
        let result = compile(PARAM_FIELD_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# field access .amount"), "parameter field access vanished from assembly:\n{}", asm);
        assert!(
            asm.contains("# cellscript abi: schema field Snapshot.amount offset=0 size=8"),
            "schema field access did not expose the concrete layout contract:\n{}",
            asm
        );
        assert!(
            asm.contains("lbu t2, 0(t4)") && asm.contains("slli t2, t2, 56"),
            "u64 schema field access did not lower to an unaligned-safe byte load sequence:\n{}",
            asm
        );
        assert!(!asm.contains("field access '.amount' has no lowered schema-backed representation"));
    }

    #[test]
    fn compile_lowers_packed_bool_and_u32_schema_fields_without_aligned_loads() {
        let result = compile(PACKED_SCALAR_FIELD_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: schema field Flags.enabled offset=0 size=1"),
            "bool schema field access did not expose concrete layout:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: schema field Flags.nonce offset=1 size=4"),
            "u32 schema field access did not expose packed Borsh offset:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: bounds check Flags.nonce required=5"),
            "packed u32 schema field access did not check full byte span:\n{}",
            asm
        );
        assert!(
            asm.contains("lbu t2, 1(t4)") && asm.contains("slli t2, t2, 24"),
            "packed u32 field access did not lower to unaligned-safe little-endian byte loads:\n{}",
            asm
        );
    }

    #[test]
    fn compile_verifies_created_output_bool_and_u32_fields() {
        let result = compile(CREATE_SCALAR_VERIFY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: verify output field Flags.enabled offset=0 size=1"),
            "created bool output field was not verified:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: verify output field Flags.nonce offset=1 size=4"),
            "created u32 output field was not verified:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: bounds check Flags.nonce required=5"),
            "created packed u32 output field did not check full byte span:\n{}",
            asm
        );
        assert!(asm.contains("li t1, 1"), "created bool expected value was not loaded as 1:\n{}", asm);
        assert!(
            asm.contains("lbu t2, 1(t4)") && asm.contains("slli t2, t2, 24"),
            "created packed u32 verifier did not use unaligned-safe byte loads:\n{}",
            asm
        );
    }

    #[test]
    fn compile_lowers_consumed_input_field_access_through_loaded_cell_bytes() {
        let result = compile(CONSUME_FIELD_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: LOAD_CELL reason=consume source=Input index=0"),
            "consume summary did not load the consumed input cell:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: bounds check Token.amount required=8"),
            "consumed input field access did not check loaded cell bounds:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: schema field Token.amount offset=0 size=8"),
            "consumed input field access did not expose concrete schema layout:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: consumed input pointer retained for verifier field checks"),
            "consume instruction destroyed the preloaded input pointer:\n{}",
            asm
        );
    }

    #[test]
    fn compile_verifies_create_output_against_consumed_input_field_alias() {
        let result = compile(CONSUME_CREATE_CONSERVATION_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: LOAD_CELL reason=consume source=Input index=0"),
            "conservation prelude did not load consumed input:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: LOAD_CELL reason=create source=Output index=0"),
            "conservation prelude did not load created output:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: verify output field Token.amount offset=0 size=8"),
            "created output amount was not verified:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: expected field Token.amount offset=0 size=8"),
            "created output amount was not compared against the consumed input amount:\n{}",
            asm
        );
        assert!(asm.contains("sub t2, t0, t1"), "created output amount and consumed input amount were not compared:\n{}", asm);
    }

    #[test]
    fn compile_verifies_create_output_against_prelude_u64_arithmetic() {
        let result = compile(CONSUME_CREATE_ARITHMETIC_CONSERVATION_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: expected expression u64 add/sub chain"),
            "created output amount was not compared against a prelude arithmetic expression:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: expected field Token.amount offset=0 size=8"),
            "prelude arithmetic expression did not read the consumed input amount:\n{}",
            asm
        );
        assert!(asm.contains("sub t1, t3, t1"), "prelude arithmetic expression did not compute input amount minus fee:\n{}", asm);
        assert!(
            asm.contains("# cellscript abi: verify output field Token.amount offset=0 size=8"),
            "created output amount was not verified:\n{}",
            asm
        );
    }

    #[test]
    fn compile_verifies_create_output_against_left_associative_prelude_u64_chain() {
        let result = compile(CONSUME_CREATE_CHAINED_ARITHMETIC_CONSERVATION_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert_eq!(
            asm.matches("# cellscript abi: expected expression u64 add/sub chain").count(),
            2,
            "left-associative prelude arithmetic chain should be recomputed recursively:\n{}",
            asm
        );
        assert!(
            asm.contains("# cellscript abi: expected field Token.amount offset=0 size=8"),
            "prelude arithmetic chain did not read the consumed input amount:\n{}",
            asm
        );
        assert!(asm.matches("sub t1, t3, t1").count() >= 2, "prelude arithmetic chain did not subtract both fee and tax:\n{}", asm);
        assert!(
            asm.contains("# cellscript abi: verify output field Token.amount offset=0 size=8"),
            "created output amount was not verified:\n{}",
            asm
        );
    }

    #[test]
    fn compile_verifies_create_output_against_local_const_prelude_u64() {
        let result = compile(CONSUME_CREATE_LOCAL_CONST_CONSERVATION_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(
            asm.contains("# cellscript abi: expected expression u64 add/sub chain"),
            "created output amount was not compared against the local-const arithmetic expression:\n{}",
            asm
        );
        assert!(asm.contains("li t1, 2"), "prelude arithmetic expression did not preserve local const fee:\n{}", asm);
        assert!(asm.contains("sub t1, t3, t1"), "prelude arithmetic expression did not subtract local const fee:\n{}", asm);
        assert!(
            asm.contains("# cellscript abi: verify output field Token.amount offset=0 size=8"),
            "created output amount was not verified:\n{}",
            asm
        );
    }

    #[test]
    fn compile_preserves_index_and_tuple_projection_in_assembly() {
        let result = compile(INDEXED_TUPLE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# index access"), "array indexing vanished from assembly:\n{}", asm);
        assert!(asm.contains("# field access .1"), "tuple projection vanished from assembly:\n{}", asm);
    }

    #[test]
    fn compile_lowers_foreach_array_into_index_length_loop_cfg() {
        let result = compile(FOREACH_ARRAY_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# length"), "foreach loop missing length lowering:\n{}", asm);
        assert!(asm.contains("# index access"), "foreach loop missing index lowering:\n{}", asm);
        assert!(asm.contains("li t0, 3"), "foreach loop missing static array length:\n{}", asm);
        assert!(asm.contains("j .Lblock_1"), "foreach loop missing back edge:\n{}", asm);
    }

    #[test]
    fn compile_lowers_len_method_to_length_instruction() {
        let result = compile(LEN_METHOD_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# length"), "len() call did not lower to length instruction:\n{}", asm);
        assert!(!asm.contains("# call len"), "len() call leaked through generic call path:\n{}", asm);
    }

    #[test]
    fn compile_lowers_match_expression_into_branch_cfg() {
        let result = compile(MATCH_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("seqz t0, t0"), "match lowering missing equality check:\n{}", asm);
        assert!(asm.contains(".Lblock_1:"), "match lowering missing first arm block:\n{}", asm);
        assert!(asm.contains(".Lblock_3:"), "match lowering missing join block:\n{}", asm);
    }

    #[test]
    fn compile_lowers_vec_builtins_without_generic_calls() {
        let result = compile(VEC_BUILTIN_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# collection new Vec"), "Vec::new() did not lower into collection instruction:\n{}", asm);
        assert!(asm.contains("# collection push"), "push() did not lower into collection instruction:\n{}", asm);
        assert!(
            asm.contains("# collection extend_from_slice"),
            "extend_from_slice() did not lower into collection instruction:\n{}",
            asm
        );
        assert!(asm.contains("# length"), "len() did not stay on builtin length path:\n{}", asm);
        assert!(!asm.contains("# call push"), "push() leaked through generic call path:\n{}", asm);
        assert!(!asm.contains("# call extend_from_slice"), "extend_from_slice() leaked through generic call path:\n{}", asm);
        assert!(!asm.contains("# call len"), "len() leaked through generic call path:\n{}", asm);
    }

    #[test]
    fn compile_lowers_type_hash_without_generic_call() {
        let result = compile(TYPE_HASH_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# type_hash"), "type_hash() did not lower into builtin instruction:\n{}", asm);
        assert!(!asm.contains("# call type_hash"), "type_hash() leaked through generic call path:\n{}", asm);
    }

    #[test]
    fn compile_lowers_zero_builtin_without_generic_call() {
        let result = compile(ZERO_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(!asm.contains("# call zero"), "Address::zero() leaked through generic call path:\n{}", asm);
        assert!(
            asm.contains("li t1, 0") || asm.contains("li t0, 0"),
            "Address::zero() did not lower to zero constant compare:\n{}",
            asm
        );
    }

    #[test]
    fn compile_result_writes_artifact_to_disk() {
        let result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        let dir = tempdir().unwrap();
        let output = Utf8Path::from_path(dir.path()).unwrap().join("out").join("program.s");

        result.write_to_path(&output).unwrap();

        let written = std::fs::read(&output).unwrap();
        assert_eq!(written, result.artifact_bytes);
    }

    #[test]
    fn compile_produces_non_empty_riscv_elf() {
        let result =
            compile(SIMPLE_PROGRAM, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();

        assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf);
        assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
        assert!(!result.artifact_bytes.is_empty());
    }

    #[test]
    fn compile_rejects_symbolic_cell_runtime_programs_as_elf() {
        let err = compile(SUMMARY_PROGRAM, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() })
            .unwrap_err();
        assert!(err.message.contains("riscv64-elf emission is not yet supported for symbolic cell/runtime operations"));
    }

    #[test]
    fn compile_lowers_schema_backed_parameter_field_access_to_elf() {
        let result =
            compile(PARAM_FIELD_PROGRAM, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() })
                .unwrap();

        assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf);
        assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
        assert!(!result.metadata.runtime.symbolic_cell_runtime_required);
        assert!(result.metadata.runtime.unsupported_elf_features.is_empty());
    }

    #[test]
    fn compile_lowers_read_ref_schema_field_to_ckb_runtime_elf() {
        let result =
            compile(READ_REF_FIELD_PROGRAM, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() })
                .unwrap();

        assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf);
        assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
        assert!(result.metadata.runtime.ckb_runtime_required);
        assert!(!result.metadata.runtime.standalone_runner_compatible);
        assert!(result.metadata.runtime.ckb_runtime_features.contains(&"read-cell-dep".to_string()));
        assert!(result.metadata.runtime.unsupported_elf_features.is_empty());
    }

    #[test]
    fn compile_rejects_symbolic_collection_programs_as_elf() {
        let err =
            compile(VEC_BUILTIN_PROGRAM, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() })
                .unwrap_err();
        assert!(err.message.contains("symbolic cell/runtime operations"));
    }

    #[test]
    fn load_modules_for_input_collects_package_source_roots() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("shared")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"
source_roots = ["src", "shared"]
"#,
        )
        .unwrap();
        std::fs::write(root.join("src/main.cell"), "module demo::main\naction ping() -> u64 { 1 }\n").unwrap();
        std::fs::write(root.join("shared/types.cell"), "module demo::types\nstruct Pair { left: u64, right: u64 }\n").unwrap();

        let modules = load_modules_for_input(root).unwrap();
        assert_eq!(modules.len(), 2);
    }

    #[test]
    fn ir_summary_captures_cell_runtime_accesses() {
        let tokens = lexer::lex(SUMMARY_PROGRAM).unwrap();
        let module = parser::parse(&tokens).unwrap();
        let ir = ir::generate(&module).unwrap();
        let action = ir
            .items
            .iter()
            .find_map(|item| match item {
                ir::IrItem::Action(action) if action.name == "update" => Some(action),
                _ => None,
            })
            .expect("update action");

        assert_eq!(action.body.read_refs.len(), 1);
        assert_eq!(action.body.create_set.len(), 1);
        assert_eq!(action.body.consume_set.len(), 1);
        assert_eq!(action.body.read_refs[0].binding, "read_ref_Config");
        assert_eq!(action.body.create_set[0].ty, "Token");
        assert!(!action.scheduler_hints.touches_shared.is_empty());
        assert!(action.scheduler_hints.estimated_cycles > 32);
    }

    #[test]
    fn compile_rejects_non_bool_lock_definitions() {
        let err = compile(BAD_LOCK_PROGRAM, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("lock definitions must return bool"));
    }

    #[test]
    fn compile_preserves_transfer_claim_settle_instructions_in_assembly() {
        let result = compile(TRANSFER_CLAIM_SETTLE_PROGRAM, CompileOptions::default()).unwrap();
        let asm = String::from_utf8(result.artifact_bytes.clone()).unwrap();

        assert!(asm.contains("# transfer"), "transfer expression vanished from assembly:\n{}", asm);
        assert!(asm.contains("# claim"), "claim expression vanished from assembly:\n{}", asm);
        assert!(asm.contains("# settle"), "settle expression vanished from assembly:\n{}", asm);
    }

    #[test]
    fn ir_summary_captures_transfer_and_claim_consumes() {
        let tokens = lexer::lex(TRANSFER_CLAIM_SETTLE_PROGRAM).unwrap();
        let module = parser::parse(&tokens).unwrap();
        let ir = ir::generate(&module).unwrap();

        let transfer_action = ir
            .items
            .iter()
            .find_map(|item| match item {
                ir::IrItem::Action(action) if action.name == "move_token" => Some(action),
                _ => None,
            })
            .expect("move_token action");
        assert_eq!(transfer_action.body.consume_set.len(), 1);
        assert_eq!(transfer_action.body.create_set.len(), 1);
        assert_eq!(transfer_action.body.consume_set[0].binding, "token");
        assert_eq!(transfer_action.body.create_set[0].ty, "Token");

        let claim_action = ir
            .items
            .iter()
            .find_map(|item| match item {
                ir::IrItem::Action(action) if action.name == "redeem" => Some(action),
                _ => None,
            })
            .expect("redeem action");
        assert_eq!(claim_action.body.consume_set.len(), 1);
        assert_eq!(claim_action.body.consume_set[0].binding, "receipt");
    }

    #[test]
    fn compile_result_exposes_scheduler_metadata_sidecar() {
        let result = compile(SUMMARY_PROGRAM, CompileOptions::default()).unwrap();
        assert_eq!(result.metadata.module, "test");
        assert_eq!(result.metadata.runtime.vm_version, "VERSION2");
        assert!(result.metadata.runtime.symbolic_cell_runtime_required);
        assert!(result.metadata.runtime.ckb_runtime_required);
        assert!(result.metadata.runtime.ckb_runtime_features.contains(&"read-cell-dep".to_string()));
        assert!(!result.metadata.runtime.unsupported_elf_features.contains(&"read-ref-expression".to_string()));
        assert!(!result.metadata.runtime.unsupported_elf_features.contains(&"schema-field-access".to_string()));
        assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| access.source == "Input"));
        assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| access.source == "CellDep"));
        assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| access.source == "Output"));
        let action = result.metadata.actions.iter().find(|action| action.name == "update").expect("update metadata");
        assert_eq!(action.read_refs.len(), 1);
        assert_eq!(action.create_set.len(), 1);
        assert_eq!(action.consume_set.len(), 1);
        assert!(!action.elf_compatible);
        assert!(action.ckb_runtime_features.contains(&"read-cell-dep".to_string()));
        assert!(!action.symbolic_runtime_features.contains(&"read-ref-expression".to_string()));
        assert!(!action.touches_shared.is_empty());
        assert!(action.estimated_cycles > 32);
        assert!(!action.scheduler_witness_borsh_hex.is_empty());
        assert!(action.scheduler_witness_borsh_hex.starts_with("11ce"));
    }

    #[test]
    fn compile_result_exposes_schema_layout_metadata() {
        let result = compile(PARAM_FIELD_PROGRAM, CompileOptions::default()).unwrap();
        let snapshot = result.metadata.types.iter().find(|ty| ty.name == "Snapshot").expect("Snapshot type metadata");
        let action = result.metadata.actions.iter().find(|action| action.name == "inspect").expect("inspect metadata");
        assert_eq!(action.params.len(), 1);
        assert_eq!(action.params[0].name, "snapshot");
        assert_eq!(action.params[0].ty, "Snapshot");
        assert!(action.params[0].schema_pointer_abi);
        assert_eq!(snapshot.kind, "Struct");
        assert_eq!(snapshot.encoded_size, Some(8));
        let amount = snapshot.fields.iter().find(|field| field.name == "amount").expect("amount field metadata");
        assert_eq!(amount.ty, "u64");
        assert_eq!(amount.offset, 0);
        assert_eq!(amount.encoded_size, Some(8));
        assert!(amount.fixed_width);
    }

    #[test]
    fn compiled_riscv_elf_contains_start_trampoline() {
        let program = r#"
module vm::smoke

action main() -> u64 {
    return 0
}
"#;

        let result =
            compile(program, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
        let dir = tempdir().unwrap();
        let output_path = Utf8Path::from_path(dir.path()).unwrap().join("main.elf");
        result.write_to_path(&output_path).unwrap();

        let Some(objdump) = find_riscv_objdump() else {
            return;
        };

        let output = Command::new(objdump).arg("-d").arg(output_path.as_str()).output().unwrap();
        assert!(output.status.success(), "objdump should disassemble generated ELF");

        let disassembly = String::from_utf8(output.stdout).unwrap();
        assert!(disassembly.contains("<_start>:"));
        assert!(disassembly.contains("<main>:"));
        assert!(disassembly.contains("a7,93"), "expected exit syscall trampoline in disassembly:\n{}", disassembly);
        assert!(disassembly.contains("a0,0"), "expected zero return value in disassembly:\n{}", disassembly);
    }

    fn find_riscv_objdump() -> Option<String> {
        let path = env::var_os("PATH")?;
        let candidates =
            ["riscv64-elf-objdump", "riscv64-unknown-elf-objdump", "riscv64-none-elf-objdump", "riscv64-linux-gnu-objdump"];

        for directory in env::split_paths(&path) {
            for candidate in candidates {
                let candidate_path = directory.join(candidate);
                if candidate_path.is_file() {
                    return Some(candidate_path.to_string_lossy().into_owned());
                }
            }
        }

        None
    }

    #[test]
    fn compile_file_loads_local_path_dependencies_from_cell_manifest() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let dep_root = root.join("dep_pkg");
        let app_root = root.join("app_pkg");

        std::fs::create_dir_all(dep_root.join("src")).unwrap();
        std::fs::create_dir_all(app_root.join("src")).unwrap();

        std::fs::write(
            dep_root.join("Cell.toml"),
            r#"
[package]
name = "dep_pkg"
version = "0.1.0"
"#,
        )
        .unwrap();
        std::fs::write(
            dep_root.join("src").join("token.cell"),
            r#"
module dep::token

resource Token has store, transfer, destroy {
    amount: u64
}
"#,
        )
        .unwrap();

        std::fs::write(
            app_root.join("Cell.toml"),
            r#"
[package]
name = "app_pkg"
version = "0.1.0"

[dependencies]
dep_pkg = { path = "../dep_pkg" }
"#,
        )
        .unwrap();
        let app_entry = app_root.join("src").join("main.cell");
        std::fs::write(
            &app_entry,
            r#"
module app::main

use dep::token::Token

action pass_through(token: Token) -> Token {
    token
}
"#,
        )
        .unwrap();

        let result = compile_file(&app_entry, CompileOptions::default()).unwrap();
        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly);
        assert!(!result.artifact_bytes.is_empty());
    }

    #[test]
    fn resolve_input_path_accepts_package_root_and_manifest() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"
"#,
        )
        .unwrap();
        let entry = root.join("src").join("main.cell");
        std::fs::write(
            &entry,
            r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
        )
        .unwrap();

        let expected: Utf8PathBuf = std::fs::canonicalize(&entry).unwrap().try_into().unwrap();
        assert_eq!(resolve_input_path(root).unwrap(), expected);
        assert_eq!(resolve_input_path(root.join("Cell.toml")).unwrap(), expected);
    }

    #[test]
    fn compile_path_accepts_package_root() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src").join("main.cell"),
            r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
        )
        .unwrap();

        let result = compile_path(root, CompileOptions::default()).unwrap();
        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly);
        assert!(!result.artifact_bytes.is_empty());
    }

    #[test]
    fn default_output_path_for_package_input_uses_build_dir() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"
"#,
        )
        .unwrap();
        let entry = root.join("src").join("main.cell");
        std::fs::write(
            &entry,
            r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
        )
        .unwrap();

        let expected = super::canonical_utf8_path(&root).unwrap().join("build").join("main.s");
        let resolved = resolve_input_path(root).unwrap();
        assert_eq!(default_output_path_for_input(root, &resolved, ArtifactFormat::RiscvAssembly).unwrap(), expected);

        let manifest = root.join("Cell.toml");
        assert_eq!(default_output_path_for_input(&manifest, &resolved, ArtifactFormat::RiscvAssembly).unwrap(), expected);
    }

    #[test]
    fn default_output_path_for_package_input_uses_manifest_out_dir() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"

[build]
out_dir = "artifacts"
"#,
        )
        .unwrap();
        let entry = root.join("src").join("main.cell");
        std::fs::write(
            &entry,
            r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
        )
        .unwrap();

        let expected = super::canonical_utf8_path(&root).unwrap().join("artifacts").join("main.s");
        let resolved = resolve_input_path(root).unwrap();
        assert_eq!(default_output_path_for_input(root, &resolved, ArtifactFormat::RiscvAssembly).unwrap(), expected);
    }

    #[test]
    fn compile_file_uses_manifest_build_target_by_default() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"

[build]
target = "riscv64-elf"
"#,
        )
        .unwrap();
        let entry = root.join("src").join("main.cell");
        std::fs::write(
            &entry,
            r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
        )
        .unwrap();

        let result = compile_file(&entry, CompileOptions::default()).unwrap();
        assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf);
        assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
    }

    #[test]
    fn compile_file_explicit_target_overrides_manifest_build_target() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"

[build]
target = "riscv64-elf"
"#,
        )
        .unwrap();
        let entry = root.join("src").join("main.cell");
        std::fs::write(
            &entry,
            r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
        )
        .unwrap();

        let result =
            compile_file(&entry, CompileOptions { target: Some("riscv64-asm".to_string()), ..CompileOptions::default() }).unwrap();
        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly);
        assert!(!result.artifact_bytes.is_empty());
    }

    #[test]
    fn compile_path_rejects_non_path_dependencies() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
token_std = "0.1.0"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src").join("main.cell"),
            r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
        )
        .unwrap();

        let err = compile_path(root, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("only local path dependencies are supported"));
        assert!(err.message.contains("token_std"));
    }

    #[test]
    fn compile_path_rejects_missing_path_dependency_manifest() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
token_std = { path = "../missing_dep" }
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src").join("main.cell"),
            r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
        )
        .unwrap();

        let err = compile_path(root, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("expected manifest"));
        assert!(err.message.contains("token_std"));
    }

    #[test]
    fn compile_path_ignores_examples_outside_package_source_roots() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("examples")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src").join("main.cell"),
            r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
        )
        .unwrap();
        std::fs::write(root.join("examples").join("broken.cell"), "this is not valid cellscript").unwrap();

        let result = compile_path(root, CompileOptions::default()).unwrap();
        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly);
        assert!(!result.artifact_bytes.is_empty());
    }

    #[test]
    fn compile_path_supports_custom_entry_directory_modules() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("contracts")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"
entry = "contracts/main.cell"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("contracts").join("helper.cell"),
            r#"
module demo::helper

resource Token has store, transfer, destroy {
    amount: u64
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("contracts").join("main.cell"),
            r#"
module demo::main

use demo::helper::Token

action pass(token: Token) -> Token {
    token
}
"#,
        )
        .unwrap();

        let result = compile_path(root, CompileOptions::default()).unwrap();
        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly);
        assert!(!result.artifact_bytes.is_empty());
    }

    #[test]
    fn compile_path_rejects_path_dependency_cycles() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let dep_root = root.join("dep_pkg");
        let app_root = root.join("app_pkg");

        std::fs::create_dir_all(dep_root.join("src")).unwrap();
        std::fs::create_dir_all(app_root.join("src")).unwrap();

        std::fs::write(
            dep_root.join("Cell.toml"),
            r#"
[package]
name = "dep_pkg"
version = "0.1.0"

[dependencies]
app_pkg = { path = "../app_pkg" }
"#,
        )
        .unwrap();
        std::fs::write(
            dep_root.join("src").join("main.cell"),
            r#"
module dep::main

action dep_ping() -> u64 {
    1
}
"#,
        )
        .unwrap();

        std::fs::write(
            app_root.join("Cell.toml"),
            r#"
[package]
name = "app_pkg"
version = "0.1.0"

[dependencies]
dep_pkg = { path = "../dep_pkg" }
"#,
        )
        .unwrap();
        std::fs::write(
            app_root.join("src").join("main.cell"),
            r#"
module app::main

action app_ping() -> u64 {
    1
}
"#,
        )
        .unwrap();

        let err = compile_path(app_root, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("path dependency cycle detected"));
    }

    #[test]
    fn compile_path_supports_configured_source_roots_without_src() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("contracts")).unwrap();
        std::fs::create_dir_all(root.join("shared")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"
entry = "contracts/main.cell"
source_roots = ["contracts", "shared"]
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("shared").join("token.cell"),
            r#"
module demo::token

resource Token has store, transfer, destroy {
    amount: u64
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("contracts").join("main.cell"),
            r#"
module demo::main

use demo::token::Token

action pass(token: Token) -> Token {
    token
}
"#,
        )
        .unwrap();

        let result = compile_path(root, CompileOptions::default()).unwrap();
        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly);
        assert!(!result.artifact_bytes.is_empty());
    }

    #[test]
    fn compile_path_rejects_missing_configured_source_root() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("contracts")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"
entry = "contracts/main.cell"
source_roots = ["contracts", "shared"]
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("contracts").join("main.cell"),
            r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
        )
        .unwrap();

        let err = compile_path(root, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("configured source root"));
        assert!(err.message.contains("shared"));
    }

    #[test]
    fn compile_path_rejects_duplicate_modules_across_source_roots() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("contracts")).unwrap();
        std::fs::create_dir_all(root.join("shared")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
name = "demo"
version = "0.1.0"
entry = "contracts/main.cell"
source_roots = ["contracts", "shared"]
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("contracts").join("main.cell"),
            r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("contracts").join("token.cell"),
            r#"
module demo::token

resource Token has store, transfer, destroy {
    amount: u64
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("shared").join("token.cell"),
            r#"
module demo::token

resource Token has store, transfer, destroy {
    amount: u64
}
"#,
        )
        .unwrap();

        let err = compile_path(root, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("duplicate module 'demo::token'"));
    }
}
