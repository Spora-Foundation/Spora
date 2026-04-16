//! CLI 子命令
//!
//! 实现各种 cellc 子命令

use crate::docgen::{DocGenerator, OutputFormat};
use crate::error::Result;
use crate::fmt::format_default;
use crate::package::PackageManager;
use crate::{
    compile_path, default_metadata_path_for_artifact, default_output_path_for_input, load_modules_for_input, resolve_input_path,
    CompileOptions,
};
use camino::Utf8Path;
#[cfg(feature = "vm-runner")]
use ckb_vm::{
    cost_model::estimate_cycles, machine::VERSION2, Bytes, DefaultCoreMachine, DefaultMachineBuilder, DefaultMachineRunner,
    SparseMemory, SupportMachine, TraceMachine, WXorXMemory, ISA_B, ISA_IMC, ISA_MOP,
};
use colored::Colorize;
use std::path::{Path, PathBuf};

/// CLI 命令
#[derive(Debug)]
pub enum Command {
    /// 编译
    Build(BuildArgs),
    /// 运行测试
    Test(TestArgs),
    /// 生成文档
    Doc(DocArgs),
    /// 格式化代码
    Fmt(FmtArgs),
    /// 初始化新项目
    Init(InitArgs),
    /// 添加依赖
    Add(AddArgs),
    /// 移除依赖
    Remove(RemoveArgs),
    /// 清理构建产物
    Clean,
    /// 运行 REPL
    Repl,
    /// 检查代码
    Check(CheckArgs),
    /// 输出 lowering/runtime 元数据
    Metadata(MetadataArgs),
    /// 运行程序
    Run(RunArgs),
    /// 发布包
    Publish(PublishArgs),
    /// 安装包
    Install(InstallArgs),
    /// 更新依赖
    Update,
    /// 显示包信息
    Info,
    /// 登录注册表
    Login(LoginArgs),
}

/// 构建参数
#[derive(Debug, Default)]
pub struct BuildArgs {
    pub release: bool,
    pub target: Option<String>,
    pub jobs: Option<usize>,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    pub verbose: bool,
}

/// 测试参数
#[derive(Debug, Default)]
pub struct TestArgs {
    pub filter: Option<String>,
    pub jobs: Option<usize>,
    pub release: bool,
    pub no_run: bool,
    pub nocapture: bool,
    pub fail_fast: bool,
    pub doc: bool,
}

/// 文档参数
#[derive(Debug, Default)]
pub struct DocArgs {
    pub open: bool,
    pub no_deps: bool,
    pub document_private_items: bool,
    pub output_format: OutputFormat,
}

/// 格式化参数
#[derive(Debug, Default)]
pub struct FmtArgs {
    pub check: bool,
    pub files: Vec<PathBuf>,
}

/// 初始化参数
#[derive(Debug, Default)]
pub struct InitArgs {
    pub name: Option<String>,
    pub path: Option<PathBuf>,
    pub lib: bool,
}

/// 添加依赖参数
#[derive(Debug, Default)]
pub struct AddArgs {
    pub crates: Vec<String>,
    pub dev: bool,
    pub build: bool,
    pub git: Option<String>,
    pub path: Option<PathBuf>,
}

/// 移除依赖参数
#[derive(Debug, Default)]
pub struct RemoveArgs {
    pub crates: Vec<String>,
    pub dev: bool,
    pub build: bool,
}

/// 检查参数
#[derive(Debug, Default)]
pub struct CheckArgs {
    pub all_targets: bool,
    pub features: Vec<String>,
}

/// 元数据参数
#[derive(Debug, Default)]
pub struct MetadataArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
}

/// 运行参数
#[derive(Debug, Default)]
pub struct RunArgs {
    pub args: Vec<String>,
    pub release: bool,
}

/// 发布参数
#[derive(Debug, Default)]
pub struct PublishArgs {
    pub dry_run: bool,
    pub allow_dirty: bool,
}

/// 安装参数
#[derive(Debug, Default)]
pub struct InstallArgs {
    pub crate_name: Option<String>,
    pub version: Option<String>,
    pub git: Option<String>,
    pub path: Option<PathBuf>,
}

/// 登录参数
#[derive(Debug, Default)]
pub struct LoginArgs {
    pub registry: Option<String>,
}

/// 命令执行器
pub struct CommandExecutor;

impl CommandExecutor {
    fn experimental_command(name: &str, detail: &str) -> Result<()> {
        Err(crate::error::CompileError::without_span(format!("cellc {} is still experimental: {}", name, detail)))
    }

    /// 执行命令
    pub fn execute(cmd: Command) -> Result<()> {
        match cmd {
            Command::Build(args) => Self::build(args),
            Command::Test(args) => Self::test(args),
            Command::Doc(args) => Self::doc(args),
            Command::Fmt(args) => Self::fmt(args),
            Command::Init(args) => Self::init(args),
            Command::Add(args) => Self::add(args),
            Command::Remove(args) => Self::remove(args),
            Command::Clean => Self::clean(),
            Command::Repl => Self::repl(),
            Command::Check(args) => Self::check(args),
            Command::Metadata(args) => Self::metadata(args),
            Command::Run(args) => Self::run(args),
            Command::Publish(args) => Self::publish(args),
            Command::Install(args) => Self::install(args),
            Command::Update => Self::update(),
            Command::Info => Self::info(),
            Command::Login(args) => Self::login(args),
        }
    }

    /// 构建项目
    fn build(args: BuildArgs) -> Result<()> {
        let opt_level = if args.release { 3 } else { 0 };
        let input = Utf8Path::new(".");
        let result = compile_path(input, CompileOptions { opt_level, output: None, debug: false, target: args.target.clone() })?;
        let resolved = resolve_input_path(input)?;
        let output_path = default_output_path_for_input(input, &resolved, result.artifact_format)?;
        result.write_to_path(&output_path)?;
        let metadata_path = default_metadata_path_for_artifact(&output_path);
        result.write_metadata_to_path(&metadata_path)?;

        println!("{}", "Build complete".green());
        println!("  Artifact format: {}", result.artifact_format.display_name());
        println!("  Output: {}", output_path);
        println!("  Metadata: {}", metadata_path);
        Ok(())
    }

    /// 运行测试
    fn test(args: TestArgs) -> Result<()> {
        if args.doc {
            Self::doc(DocArgs { output_format: OutputFormat::Markdown, ..Default::default() })?;
        }

        let mut test_inputs = collect_cell_files(Path::new("tests"))?;
        if let Some(filter) = &args.filter {
            test_inputs.retain(|path| path.to_string_lossy().contains(filter));
        }
        test_inputs.sort();

        if test_inputs.is_empty() {
            compile_path(".", CompileOptions { opt_level: 0, output: None, debug: false, target: None })?;
            println!("{}", "Test compile complete".green());
            println!("  Package check: passed");
            println!("  Test files: 0");
            if !args.no_run {
                println!("  Execution: skipped; no CellScript test files were found");
            }
            return Ok(());
        }

        let mut passed = 0usize;
        for input in &test_inputs {
            let utf8 = Utf8Path::from_path(input)
                .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input.display())))?;
            if args.nocapture {
                println!("  Compiling {}", utf8);
            }
            compile_path(utf8, CompileOptions { opt_level: 0, output: None, debug: false, target: None })?;
            passed += 1;
        }

        println!("{}", "Test compile complete".green());
        println!("  Compiled {} test file(s)", passed);
        if !args.no_run {
            println!("  Execution: skipped; CellScript test execution is not enabled in the default toolchain yet");
        }
        Ok(())
    }

    /// 生成文档
    fn doc(args: DocArgs) -> Result<()> {
        let modules = load_modules_for_input(".")?;
        let mut generator = DocGenerator::new(args.output_format);
        for module in &modules {
            generator.add_module(&module.ast);
        }
        let docs = generator.generate()?;
        let output = match args.output_format {
            OutputFormat::Html => PathBuf::from("docs/cellscript-api.html"),
            OutputFormat::Markdown => PathBuf::from("docs/cellscript-api.md"),
            OutputFormat::Json => PathBuf::from("docs/cellscript-api.json"),
        };
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&output, docs)?;

        println!("{}", "Documentation generated".green());
        println!("  Output: {}", output.display());

        if args.open {
            let _ = std::process::Command::new("open").arg(&output).status();
        }

        Ok(())
    }

    /// 格式化代码
    fn fmt(args: FmtArgs) -> Result<()> {
        let modules = if args.files.is_empty() {
            load_modules_for_input(".")?
        } else {
            let mut modules = Vec::new();
            for path in &args.files {
                let utf8 = Utf8Path::from_path(path).ok_or_else(|| {
                    crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", path.display()))
                })?;
                modules.extend(load_modules_for_input(utf8)?);
            }
            modules
        };

        let mut changed = Vec::new();
        for module in modules {
            let formatted = format_default(&module.ast)?;
            if formatted != module.source {
                changed.push(module.path.clone());
                if !args.check {
                    std::fs::write(&module.path, formatted)?;
                }
            }
        }

        if args.check {
            if changed.is_empty() {
                println!("{}", "Formatting is clean".green());
                Ok(())
            } else {
                Err(crate::error::CompileError::without_span(format!(
                    "format check failed for {} file(s): {}",
                    changed.len(),
                    changed.iter().map(|path| path.as_str()).collect::<Vec<_>>().join(", ")
                )))
            }
        } else {
            println!("{}", "Formatting complete".green());
            println!("  Updated {} file(s)", changed.len());
            Ok(())
        }
    }

    /// 初始化新项目
    fn init(args: InitArgs) -> Result<()> {
        let path = args.path.unwrap_or_else(|| PathBuf::from("."));
        let name = args.name.unwrap_or_else(|| path.file_name().unwrap_or_default().to_string_lossy().to_string());

        println!("{} {} in {}", "Creating".cyan(), if args.lib { "library" } else { "binary" }, path.display());

        let pm = PackageManager::new(&path);
        pm.init(&name)?;

        if args.lib {
            // 创建 lib.cell
            std::fs::write(path.join("src/lib.cell"), format!("module {};\n", name))?;
        }

        println!("{}", "Created package successfully".green());
        println!("  To get started:");
        println!("    cd {}", path.display());
        println!("    cellc build");

        Ok(())
    }

    /// 添加依赖
    fn add(args: AddArgs) -> Result<()> {
        let pm = PackageManager::new(".");

        for crate_name in &args.crates {
            println!("{} {} to dependencies", "Adding".cyan(), crate_name);

            let version = "*".to_string(); // 默认使用最新版本
            pm.add_dependency(crate_name, &version)?;
        }

        println!("{}", "Dependencies added successfully".green());
        Ok(())
    }

    /// 移除依赖
    fn remove(args: RemoveArgs) -> Result<()> {
        let pm = PackageManager::new(".");

        for crate_name in &args.crates {
            println!("{} {} from dependencies", "Removing".cyan(), crate_name);
            pm.remove_dependency(crate_name)?;
        }

        println!("{}", "Dependencies removed successfully".green());
        Ok(())
    }

    /// 清理构建产物
    fn clean() -> Result<()> {
        println!("{}", "Cleaning...".cyan());

        let paths = vec!["target", ".cell/cache"];

        for path in paths {
            if std::path::Path::new(path).exists() {
                println!("  Removing {}", path);
                std::fs::remove_dir_all(path)?;
            }
        }

        println!("{}", "Clean complete".green());
        Ok(())
    }

    /// 运行 REPL
    fn repl() -> Result<()> {
        crate::repl::run_repl().map_err(|e| crate::error::CompileError::without_span(e.to_string()))
    }

    /// 检查代码
    fn check(args: CheckArgs) -> Result<()> {
        let requested_target = if args.all_targets { Some("riscv64-elf".to_string()) } else { None };
        compile_path(".", CompileOptions { opt_level: 0, output: None, debug: false, target: requested_target })?;
        println!("{}", "Check succeeded".green());
        Ok(())
    }

    /// 输出 lowering/runtime 元数据
    fn metadata(args: MetadataArgs) -> Result<()> {
        let input_path = args.input.unwrap_or_else(|| PathBuf::from("."));
        let input = Utf8Path::from_path(&input_path)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
        let result = compile_path(input, CompileOptions { opt_level: 0, output: None, debug: false, target: args.target })?;
        let json = serde_json::to_string_pretty(&result.metadata)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize metadata: {}", error)))?;

        if let Some(output_path) = args.output {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, json)?;
            println!("{}", "Metadata generated".green());
            println!("  Output: {}", output_path.display());
        } else {
            println!("{}", json);
        }
        Ok(())
    }

    /// 运行程序
    fn run(args: RunArgs) -> Result<()> {
        #[cfg(feature = "vm-runner")]
        {
            let opt_level = if args.release { 3 } else { 0 };
            let result =
                compile_path(".", CompileOptions { opt_level, output: None, debug: false, target: Some("riscv64-elf".to_string()) })?;
            let parameterized_entries = result
                .metadata
                .actions
                .iter()
                .filter(|action| !action.params.is_empty())
                .map(|action| format!("action {}", action.name))
                .chain(result.metadata.locks.iter().filter(|lock| !lock.params.is_empty()).map(|lock| format!("lock {}", lock.name)))
                .collect::<Vec<_>>();
            if !parameterized_entries.is_empty() {
                return Err(crate::error::CompileError::without_span(format!(
                    "cellc run only supports no-argument pure ELF entrypoints today; {} requires a transaction/parameter ABI context",
                    parameterized_entries.join(", ")
                )));
            }
            if result.metadata.runtime.ckb_runtime_required {
                return Err(crate::error::CompileError::without_span(format!(
                    "cellc run cannot provide CKB transaction/syscall context; required runtime features: {}",
                    result.metadata.runtime.ckb_runtime_features.join(", ")
                )));
            }
            if !result.metadata.runtime.standalone_runner_compatible {
                return Err(crate::error::CompileError::without_span(
                    "cellc run only supports standalone-compatible ELF without symbolic Cell/runtime requirements",
                ));
            }
            let vm_args = args.args.into_iter().map(|arg| arg.into_bytes()).collect::<Vec<_>>();
            let cycles = run_elf_in_ckb_vm(&result.artifact_bytes, &vm_args)?;

            println!("{}", "Run complete".green());
            println!("  Artifact format: {}", result.artifact_format.display_name());
            println!("  Cycles: {}", cycles);
            Ok(())
        }

        #[cfg(not(feature = "vm-runner"))]
        {
            let release = if args.release { "release" } else { "debug" };
            Self::experimental_command(
            "run",
            &format!(
                "trusted CKB-VM execution must be provided by a separate runner or feature-gated VM backend (requested {} mode with {} argument(s))",
                release,
                args.args.len()
            ),
        )
        }
    }

    /// 发布包
    fn publish(args: PublishArgs) -> Result<()> {
        let mode = if args.dry_run { "dry-run" } else { "publish" };
        let dirty = if args.allow_dirty { "allow-dirty" } else { "clean-tree-only" };
        Self::experimental_command(
            "publish",
            &format!("registry publication flow is not implemented yet (requested {}, {})", mode, dirty),
        )
    }

    /// 安装包
    fn install(args: InstallArgs) -> Result<()> {
        let source = args.crate_name.unwrap_or_else(|| "current directory".to_string());
        Self::experimental_command(
            "install",
            &format!("package installation flow is not implemented yet (requested source '{}')", source),
        )
    }

    /// 更新依赖
    fn update() -> Result<()> {
        Self::experimental_command("update", "dependency resolution and lockfile update flow are not implemented yet")
    }

    /// 显示包信息
    fn info() -> Result<()> {
        let pm = PackageManager::new(".");
        let manifest = pm.read_manifest()?;

        println!("{}", "Package Info:".bold());
        println!("  Name:        {}", manifest.package.name);
        println!("  Version:     {}", manifest.package.version);
        println!("  Description: {}", manifest.package.description);
        println!("  License:     {}", manifest.package.license);
        println!("  Authors:     {}", manifest.package.authors.join(", "));
        println!("  Entry:       {}", manifest.package.entry);
        println!("  Dependencies:");
        for (name, dep) in &manifest.dependencies {
            println!("    - {}: {:?}", name, dep);
        }

        Ok(())
    }

    /// 登录注册表
    fn login(args: LoginArgs) -> Result<()> {
        let registry = args.registry.unwrap_or_else(|| "https://cellscript.io".to_string());
        Self::experimental_command("login", &format!("registry auth flow is not implemented yet (requested '{}')", registry))
    }
}

#[cfg(feature = "vm-runner")]
type CliVmMachine = TraceMachine<DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>>;

fn collect_cell_files(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    if root.is_file() {
        return Ok(if root.extension().and_then(|ext| ext.to_str()) == Some("cell") { vec![root.to_path_buf()] } else { Vec::new() });
    }

    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("cell") {
                files.push(path);
            }
        }
    }
    Ok(files)
}

#[cfg(feature = "vm-runner")]
fn run_elf_in_ckb_vm(program: &[u8], args: &[Vec<u8>]) -> Result<u64> {
    let core_machine =
        <<CliVmMachine as DefaultMachineRunner>::Inner as SupportMachine>::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 10_000_000);
    let builder = DefaultMachineBuilder::new(core_machine).instruction_cycle_func(Box::new(estimate_cycles));
    let mut machine = CliVmMachine::new(builder.build());
    let program = Bytes::copy_from_slice(program);
    let args = args.iter().cloned().map(Bytes::from).map(Ok);

    machine
        .load_program(&program, args)
        .map_err(|error| crate::error::CompileError::without_span(format!("cellc run failed to load ELF: {}", error)))?;
    let exit_code =
        machine.run().map_err(|error| crate::error::CompileError::without_span(format!("cellc run VM error: {}", error)))?;
    if exit_code != 0 {
        return Err(crate::error::CompileError::without_span(format!("cellc run exited with code {}", exit_code)));
    }

    Ok(machine.machine.cycles())
}

/// 命令行解析
pub struct CliParser;

impl CliParser {
    /// 解析命令行参数
    pub fn parse() -> Command {
        use clap::{Arg, ArgAction, Command as ClapCommand};

        let matches = ClapCommand::new("cellc")
            .version(crate::VERSION)
            .about("CellScript compiler for Spora blockchain")
            .subcommand_required(true)
            .arg_required_else_help(true)
            .subcommand(
                ClapCommand::new("build")
                    .about("Compile the current package")
                    .arg(Arg::new("release").long("release").short('r').action(ArgAction::SetTrue).help("Build in release mode"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("jobs").long("jobs").short('j').value_name("N").help("Number of parallel jobs")),
            )
            .subcommand(
                ClapCommand::new("test")
                    .about("Run the tests")
                    .arg(Arg::new("filter").value_name("FILTER").help("Filter tests by name"))
                    .arg(
                        Arg::new("no-run")
                            .long("no-run")
                            .action(ArgAction::SetTrue)
                            .help("Compile tests without attempting execution"),
                    )
                    .arg(Arg::new("nocapture").long("nocapture").action(ArgAction::SetTrue).help("Don't capture stdout"))
                    .arg(Arg::new("fail-fast").long("fail-fast").action(ArgAction::SetTrue).help("Stop on first failure"))
                    .arg(Arg::new("doc").long("doc").action(ArgAction::SetTrue).help("Generate docs before compiling tests")),
            )
            .subcommand(
                ClapCommand::new("doc")
                    .about("Generate documentation")
                    .arg(Arg::new("open").long("open").short('o').action(ArgAction::SetTrue).help("Open docs in browser"))
                    .arg(
                        Arg::new("format")
                            .long("format")
                            .value_name("FORMAT")
                            .default_value("html")
                            .help("Output format: html, markdown, json"),
                    ),
            )
            .subcommand(
                ClapCommand::new("fmt")
                    .about("Format source code")
                    .arg(Arg::new("check").long("check").action(ArgAction::SetTrue).help("Check formatting without modifying files"))
                    .arg(Arg::new("files").value_name("FILES").num_args(1..).help("Files to format")),
            )
            .subcommand(
                ClapCommand::new("init")
                    .about("Create a new package")
                    .arg(Arg::new("name").value_name("NAME").help("Package name"))
                    .arg(Arg::new("path").value_name("PATH").help("Path to create package"))
                    .arg(Arg::new("lib").long("lib").action(ArgAction::SetTrue).help("Create a library package")),
            )
            .subcommand(
                ClapCommand::new("add")
                    .about("Add dependencies")
                    .arg(Arg::new("crates").value_name("CRATES").required(true).num_args(1..).help("Crates to add"))
                    .arg(Arg::new("dev").long("dev").action(ArgAction::SetTrue).help("Add as dev dependency")),
            )
            .subcommand(ClapCommand::new("clean").about("Remove build artifacts"))
            .subcommand(
                ClapCommand::new("remove")
                    .about("Remove dependencies")
                    .arg(Arg::new("crates").value_name("CRATES").required(true).num_args(1..).help("Crates to remove"))
                    .arg(Arg::new("dev").long("dev").action(ArgAction::SetTrue).help("Remove from dev dependency section")),
            )
            .subcommand(ClapCommand::new("repl").about("Start interactive REPL"))
            .subcommand(
                ClapCommand::new("check").about("Type-check and lower the current package without writing artifacts").arg(
                    Arg::new("all-targets")
                        .long("all-targets")
                        .action(ArgAction::SetTrue)
                        .help("Also check the current ELF-compatible target path"),
                ),
            )
            .subcommand(
                ClapCommand::new("metadata")
                    .about("Emit compile metadata for lowering, scheduler, and CKB runtime auditing")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON metadata to a file"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture")),
            )
            .subcommand(
                ClapCommand::new("run")
                    .about("Experimental: build and run a package")
                    .arg(Arg::new("release").long("release").short('r').action(ArgAction::SetTrue).help("Run in release mode"))
                    .arg(Arg::new("args").value_name("ARGS").num_args(0..).trailing_var_arg(true)),
            )
            .subcommand(
                ClapCommand::new("publish")
                    .about("Experimental: publish a package")
                    .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
                    .arg(Arg::new("allow-dirty").long("allow-dirty").action(ArgAction::SetTrue)),
            )
            .subcommand(
                ClapCommand::new("install")
                    .about("Experimental: install a package")
                    .arg(Arg::new("crate").value_name("CRATE"))
                    .arg(Arg::new("version").long("version").value_name("VERSION"))
                    .arg(Arg::new("git").long("git").value_name("URL"))
                    .arg(Arg::new("path").long("path").value_name("PATH")),
            )
            .subcommand(ClapCommand::new("update").about("Experimental: update dependencies"))
            .subcommand(ClapCommand::new("info").about("Show package information"))
            .subcommand(
                ClapCommand::new("login")
                    .about("Experimental: authenticate against a registry")
                    .arg(Arg::new("registry").long("registry").value_name("URL")),
            )
            .get_matches();

        match matches.subcommand() {
            Some(("build", m)) => Command::Build(BuildArgs {
                release: m.get_flag("release"),
                target: m.get_one::<String>("target").cloned(),
                jobs: m.get_one::<String>("jobs").and_then(|s| s.parse().ok()),
                ..Default::default()
            }),
            Some(("test", m)) => Command::Test(TestArgs {
                filter: m.get_one::<String>("filter").cloned(),
                no_run: m.get_flag("no-run"),
                nocapture: m.get_flag("nocapture"),
                fail_fast: m.get_flag("fail-fast"),
                doc: m.get_flag("doc"),
                ..Default::default()
            }),
            Some(("doc", m)) => Command::Doc(DocArgs {
                open: m.get_flag("open"),
                output_format: match m.get_one::<String>("format").map(|s| s.as_str()) {
                    Some("markdown") => OutputFormat::Markdown,
                    Some("json") => OutputFormat::Json,
                    _ => OutputFormat::Html,
                },
                ..Default::default()
            }),
            Some(("fmt", m)) => Command::Fmt(FmtArgs {
                check: m.get_flag("check"),
                files: m.get_many::<String>("files").map(|v| v.map(PathBuf::from).collect()).unwrap_or_default(),
            }),
            Some(("init", m)) => Command::Init(InitArgs {
                name: m.get_one::<String>("name").cloned(),
                path: m.get_one::<String>("path").map(PathBuf::from),
                lib: m.get_flag("lib"),
            }),
            Some(("add", m)) => Command::Add(AddArgs {
                crates: m.get_many::<String>("crates").map(|v| v.cloned().collect()).unwrap_or_default(),
                dev: m.get_flag("dev"),
                ..Default::default()
            }),
            Some(("remove", m)) => Command::Remove(RemoveArgs {
                crates: m.get_many::<String>("crates").map(|v| v.cloned().collect()).unwrap_or_default(),
                dev: m.get_flag("dev"),
                build: false,
            }),
            Some(("clean", _)) => Command::Clean,
            Some(("repl", _)) => Command::Repl,
            Some(("check", m)) => Command::Check(CheckArgs { all_targets: m.get_flag("all-targets"), features: Vec::new() }),
            Some(("metadata", m)) => Command::Metadata(MetadataArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
            }),
            Some(("run", m)) => Command::Run(RunArgs {
                args: m.get_many::<String>("args").map(|values| values.cloned().collect()).unwrap_or_default(),
                release: m.get_flag("release"),
            }),
            Some(("publish", m)) => {
                Command::Publish(PublishArgs { dry_run: m.get_flag("dry-run"), allow_dirty: m.get_flag("allow-dirty") })
            }
            Some(("install", m)) => Command::Install(InstallArgs {
                crate_name: m.get_one::<String>("crate").cloned(),
                version: m.get_one::<String>("version").cloned(),
                git: m.get_one::<String>("git").cloned(),
                path: m.get_one::<String>("path").map(PathBuf::from),
            }),
            Some(("update", _)) => Command::Update,
            Some(("info", _)) => Command::Info,
            Some(("login", m)) => Command::Login(LoginArgs { registry: m.get_one::<String>("registry").cloned() }),
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_execution() {
        // 测试命令执行
        let _cmd = Command::Clean;
        // 实际测试需要 mock 文件系统
    }
}
