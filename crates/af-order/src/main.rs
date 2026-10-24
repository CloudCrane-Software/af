//! af-order 下单门面 CLI 薄壳：参数 → [`af_order::cli::run`] → stdout /
//! stderr / 退出码。语义全部在库内（可测），main 只做进程接线。

use af_order::cli;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::run(&args) {
        Ok(stdout) => println!("{stdout}"),
        Err(err) => {
            eprintln!("af-order: {err}");
            std::process::exit(err.exit_code());
        }
    }
}
