//! CLI 模块
//!
//! 命令行界面和子命令实现

pub mod commands;

use commands::{Command, CommandExecutor, CliParser};
use crate::error::Result;

/// 运行 CLI
pub fn run() -> Result<()> {
    let cmd = CliParser::parse();
    CommandExecutor::execute(cmd)
}
