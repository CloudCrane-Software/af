//! 一次进程只处理 stdin 的第一行，写一行 JSON 到 stdout，然后退出。
//! 不循环：进程内存里不保留上一张订单。

use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut line = String::new();
    let response = match io::stdin().read_line(&mut line) {
        Ok(_) => af_engine::handle_line(&line),
        Err(e) => af_engine::bad_request(&format!("stdin: {e}")),
    };
    let mut stdout = io::stdout();
    if writeln!(stdout, "{response}").is_err() {
        return ExitCode::FAILURE;
    }
    if stdout.flush().is_err() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
