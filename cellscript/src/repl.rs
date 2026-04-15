//! CellScript REPL (交互式解释器)
//!
//! 提供交互式 CellScript 编程环境

use crate::codegen::{self, CodegenOptions};
use crate::ir::generate;
use crate::lexer::lex;
use crate::parser::parse;
use crate::types::check;
use colored::Colorize;
use std::io::{self, Write};

/// REPL 状态
pub struct Repl {
    /// 历史输入
    history: Vec<String>,
    /// 当前模块上下文
    context: String,
    /// 是否显示生成的 IR
    show_ir: bool,
    /// 是否显示生成的汇编
    show_asm: bool,
}

impl Repl {
    /// 创建新的 REPL
    pub fn new() -> Self {
        Self { history: Vec::new(), context: String::new(), show_ir: false, show_asm: false }
    }

    /// 运行 REPL
    pub fn run(&mut self) -> io::Result<()> {
        self.print_banner();

        let stdin = io::stdin();
        let mut stdout = io::stdout();

        loop {
            print!("{} ", "cellc>".cyan().bold());
            stdout.flush()?;

            let mut input = String::new();
            stdin.read_line(&mut input)?;

            let input = input.trim();
            if input.is_empty() {
                continue;
            }

            self.history.push(input.to_string());

            // 处理特殊命令
            if input.starts_with(':') {
                if self.handle_command(input) {
                    break;
                }
                continue;
            }

            // 处理代码输入
            if let Err(e) = self.process_input(input) {
                eprintln!("{}: {}", "error".red(), e);
            }
        }

        Ok(())
    }

    /// 打印欢迎横幅
    fn print_banner(&self) {
        println!(
            "{}",
            r#"
   ____     _       _   _           _   
  / ___|__| | ___ | |_| |__   ___ | |_ 
 | |   / _` |/ _ \| __| '_ \ / _ \| __|
 | |__| (_| | (_) | |_| | | | (_) | |_ 
  \____\__,_|\___/ \__|_| |_|\___/ \__|
                                       
        CellScript Interactive Shell
              Version 0.1.0
"#
            .cyan()
        );
        println!("Type {} for help, {} to exit\n", ":help".yellow(), ":quit".yellow());
    }

    /// 处理特殊命令
    /// 返回 true 表示退出 REPL
    fn handle_command(&mut self, input: &str) -> bool {
        let parts: Vec<&str> = input.split_whitespace().collect();
        let cmd = parts[0];

        match cmd {
            ":quit" | ":q" => {
                println!("Goodbye!");
                true
            }
            ":help" | ":h" => {
                self.print_help();
                false
            }
            ":history" => {
                for (i, line) in self.history.iter().enumerate() {
                    println!("  {}: {}", i + 1, line);
                }
                false
            }
            ":clear" => {
                self.context.clear();
                println!("Context cleared.");
                false
            }
            ":show" => {
                if parts.len() > 1 {
                    match parts[1] {
                        "ir" => {
                            self.show_ir = !self.show_ir;
                            println!("Show IR: {}", if self.show_ir { "on" } else { "off" });
                        }
                        "asm" => {
                            self.show_asm = !self.show_asm;
                            println!("Show ASM: {}", if self.show_asm { "on" } else { "off" });
                        }
                        _ => println!("Unknown show option: {}", parts[1]),
                    }
                }
                false
            }
            ":lex" => {
                if parts.len() > 1 {
                    let code = parts[1..].join(" ");
                    self.show_tokens(&code);
                }
                false
            }
            ":parse" => {
                if parts.len() > 1 {
                    let code = parts[1..].join(" ");
                    self.show_ast(&code);
                }
                false
            }
            _ => {
                println!("Unknown command: {}. Type :help for help.", cmd);
                false
            }
        }
    }

    /// 打印帮助信息
    fn print_help(&self) {
        println!("{}", "Commands:".bold());
        println!("  {:15} - Exit the REPL", ":quit, :q".yellow());
        println!("  {:15} - Show this help message", ":help, :h".yellow());
        println!("  {:15} - Show input history", ":history".yellow());
        println!("  {:15} - Clear current context", ":clear".yellow());
        println!("  {:15} - Toggle IR display", ":show ir".yellow());
        println!("  {:15} - Toggle ASM display", ":show asm".yellow());
        println!("  {:15} - Tokenize code", ":lex <code>".yellow());
        println!("  {:15} - Parse code to AST", ":parse <code>".yellow());
        println!();
        println!("{}", "Example code:".bold());
        println!("  let x = 42");
        println!("  resource Token {{ amount: u64 }}");
        println!("  action mint() {{ create Token {{ amount: 100 }} }}");
    }

    /// 处理代码输入
    fn process_input(&mut self, input: &str) -> Result<(), String> {
        // 构建完整代码（添加上下文）
        let full_code = if self.context.is_empty() {
            format!("module repl\n{}", input)
        } else {
            format!("module repl\n{}\n{}", self.context, input)
        };

        // 1. 词法分析
        let tokens = lex(&full_code).map_err(|e| format!("Lexer error: {}", e))?;

        // 2. 解析
        let ast = parse(&tokens).map_err(|e| format!("Parser error: {}", e))?;

        // 3. 类型检查
        check(&ast).map_err(|e| format!("Type error: {}", e))?;

        // 4. 生成 IR
        let ir = generate(&ast).map_err(|e| format!("IR generation error: {}", e))?;

        println!("{}", "✓".green().bold());

        // 显示 IR（如果启用）
        if self.show_ir {
            println!("{}\n{:#?}", "Generated IR:".cyan().bold(), ir);
        }

        // 显示汇编（如果启用）
        if self.show_asm {
            let asm_bytes = codegen::generate(&ir, &CodegenOptions::default(), crate::ArtifactFormat::RiscvAssembly)
                .map_err(|e| format!("Codegen error: {}", e))?;
            let asm = String::from_utf8(asm_bytes).map_err(|e| format!("Assembly output is not valid UTF-8: {}", e))?;
            println!("{}\n{}", "Generated ASM:".cyan().bold(), asm);
        }

        // 更新上下文
        if !input.starts_with("action") && !input.starts_with("resource") {
            self.context.push_str(input);
            self.context.push('\n');
        }

        Ok(())
    }

    /// 显示 token
    fn show_tokens(&self, code: &str) {
        let full_code = format!("module repl\n{}", code);
        match lex(&full_code) {
            Ok(tokens) => {
                println!("{}", "Tokens:".cyan().bold());
                for token in tokens {
                    if !matches!(
                        token.kind,
                        crate::lexer::token::TokenKind::Whitespace
                            | crate::lexer::token::TokenKind::Newline
                            | crate::lexer::token::TokenKind::Eof
                    ) {
                        println!("  {:?}", token);
                    }
                }
            }
            Err(e) => eprintln!("{}: {}", "Error".red(), e),
        }
    }

    /// 显示 AST
    fn show_ast(&self, code: &str) {
        let full_code = format!("module repl\n{}", code);
        match lex(&full_code) {
            Ok(tokens) => match parse(&tokens) {
                Ok(ast) => {
                    println!("{}", "AST:".cyan().bold());
                    println!("{:#?}", ast);
                }
                Err(e) => eprintln!("{}: {}", "Parse error".red(), e),
            },
            Err(e) => eprintln!("{}: {}", "Lexer error".red(), e),
        }
    }
}

/// 运行 REPL
pub fn run_repl() -> io::Result<()> {
    let mut repl = Repl::new();
    repl.run()
}
