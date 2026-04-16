//! CellScript CLI 编译器

use camino::Utf8Path;
use clap::Parser;
use colored::Colorize;
use std::process;

use cellscript::{
    compile_path, default_metadata_path_for_artifact, default_output_path_for_input, resolve_input_path, CompileOptions,
};

/// CellScript 编译器
#[derive(Parser, Debug)]
#[command(name = "cellc")]
#[command(about = "CellScript compiler for Spora blockchain")]
#[command(version = cellscript::VERSION)]
struct Cli {
    /// 输入文件、包目录或 Cell.toml
    #[arg(value_name = "INPUT")]
    input: Option<String>,

    /// 优化级别 (0-3)
    #[arg(short = 'O', long, default_value = "0")]
    opt: u8,

    /// 输出文件
    #[arg(short, long, value_name = "FILE")]
    output: Option<String>,

    /// 生成调试信息
    #[arg(short, long)]
    debug: bool,

    /// 目标产物 (asm, riscv64-asm, riscv64-elf)
    #[arg(short, long)]
    target: Option<String>,

    /// 仅词法分析
    #[arg(long)]
    lex: bool,

    /// 仅解析
    #[arg(long)]
    parse: bool,

    /// 交互式 REPL 模式
    #[arg(short, long)]
    interactive: bool,

    /// 生成标准库汇编
    #[arg(long)]
    gen_stdlib: bool,
}

fn main() {
    if std::env::args()
        .nth(1)
        .map(|arg| {
            matches!(
                arg.as_str(),
                "build"
                    | "test"
                    | "doc"
                    | "fmt"
                    | "init"
                    | "add"
                    | "remove"
                    | "clean"
                    | "repl"
                    | "check"
                    | "metadata"
                    | "run"
                    | "publish"
                    | "install"
                    | "update"
                    | "info"
                    | "login"
            )
        })
        .unwrap_or(false)
    {
        if let Err(e) = cellscript::cli::run() {
            eprintln!("{}: {}", "error".red(), e);
            process::exit(1);
        }
        return;
    }

    let cli = Cli::parse();

    // 设置日志
    env_logger::init();

    // REPL 模式
    if cli.interactive {
        if let Err(e) = cellscript::repl::run_repl() {
            eprintln!("{}: {}", "REPL error".red(), e);
            process::exit(1);
        }
        return;
    }

    // 生成标准库
    if cli.gen_stdlib {
        let asm = cellscript::stdlib::StdLib::generate_assembly();
        println!("{}", asm);
        return;
    }

    // 验证优化级别
    if cli.opt > 3 {
        eprintln!("{}: optimization level must be between 0 and 3", "error".red());
        process::exit(1);
    }

    // 获取输入文件
    let input_file = cli.input.unwrap_or_else(|| ".".to_string());
    let resolved_input = match resolve_input_path(Utf8Path::new(&input_file)) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("{}: {}", "error".red(), e);
            process::exit(1);
        }
    };

    // 读取输入文件
    let source = match std::fs::read_to_string(&resolved_input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: failed to read '{}': {}", "error".red(), resolved_input, e);
            process::exit(1);
        }
    };

    // 仅词法分析模式
    if cli.lex {
        match cellscript::lexer::lex(&source) {
            Ok(tokens) => {
                println!("{}: found {} tokens", "success".green(), tokens.len());
                for token in tokens {
                    println!("  {:?}", token);
                }
            }
            Err(e) => {
                eprintln!("{}: {}", "error".red(), e);
                process::exit(1);
            }
        }
        return;
    }

    // 仅解析模式
    if cli.parse {
        let tokens = match cellscript::lexer::lex(&source) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{}: {}", "error".red(), e);
                process::exit(1);
            }
        };

        match cellscript::parser::parse(&tokens) {
            Ok(ast) => {
                println!("{}: parsed successfully", "success".green());
                println!("{:#?}", ast);
            }
            Err(e) => {
                eprintln!("{}: {}", "error".red(), e);
                process::exit(1);
            }
        }
        return;
    }

    // 完整编译
    let output = cli.output.clone();
    let options = CompileOptions { opt_level: cli.opt, output: output.clone(), debug: cli.debug, target: cli.target };

    match compile_path(Utf8Path::new(&input_file), options) {
        Ok(result) => {
            let output_path = output
                .as_deref()
                .map(Utf8Path::new)
                .map(|path| path.to_owned())
                .map(Ok)
                .unwrap_or_else(|| default_output_path_for_input(Utf8Path::new(&input_file), &resolved_input, result.artifact_format))
                .unwrap_or_else(|e| {
                    eprintln!("{}: {}", "error".red(), e);
                    process::exit(1);
                });

            if let Err(e) = result.write_to_path(&output_path) {
                eprintln!("{}: {}", "error".red(), e);
                process::exit(1);
            }
            let metadata_path = default_metadata_path_for_artifact(&output_path);
            if let Err(e) = result.write_metadata_to_path(&metadata_path) {
                eprintln!("{}: {}", "error".red(), e);
                process::exit(1);
            }

            println!("{}: compiled successfully", "success".green());
            println!("  Artifact format: {}", result.artifact_format.display_name());
            println!("  Artifact hash: {:x?}", result.artifact_hash);
            println!("  Output: {}", output_path);
            println!("  Metadata: {}", metadata_path);
        }
        Err(e) => {
            eprintln!("{}: {}", "error".red(), e);
            process::exit(1);
        }
    }
}
