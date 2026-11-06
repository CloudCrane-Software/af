//! `af-compiler-construct`：建设任务包组包入口（E1 缺口①②④的 CLI 面）。
//!
//! 一跳：四段全链编译内置建设图 → plan.json（提交形态）+ task.json
//! （任务书内嵌 declare/envelope 协议教学段）+ package_manifest.json
//! （内容 hash 留档，改动须清态换号重提）落 <out_dir>，stdout 留证 JSON。
//! 库内正本 = [`af_compiler::construction`]；失败 stderr 点名 + 非零退出码。

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    af_compiler::construction::main(&args)
}
