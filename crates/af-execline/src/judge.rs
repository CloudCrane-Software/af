//! 判分器定位与调用——判分子进程的唯一调用点（af-bench 吸收方案 §3）。
//!
//! 独立性边界 = 子进程边界（§3.2-1）：af-execline 永不进程内链接/复刻判分逻辑，
//! 判分的一切调用收敛到本模块。argv 契约钉死——判分脚本恰好收到三个位置参数
//! `[task_json, eval.log, agent.diff]`、不经 wrapper shell（`Command` 直指解释
//! 器）——实现面（提示词/循环/工具）怎么改都碰不到判分。解释器旗标与 env 只
//! 有判分执行面防线（红队 W3-C34）：`-B` + `PYTHONDONTWRITEBYTECODE=1` +
//! 受控空 `PYTHONPYCACHEPREFIX`，除此之外零注入。契约由 tests/p3_judge.rs 假
//! 判分 venv 行为断言 + 源文本比对双向钉死。
//!
//! 指纹门 fail-loud（§3.2-2）：`sha256(判分脚本)` 必须等于声明锚（默认 = 本机
//! 现产值 `a292547f…`，§3.1 实测；历史回放对账构造 [`JudgeLocator`] 指向
//! rsi-drill 冻结件 + `0338566c…` 锚即可，方案双锚口径）。不匹配 = EX3002 拒跑，
//! 不降级、不跳判、不静默换脚本。
//!
//! 导入链信任锚（红队 W3-C34）：真判分逻辑不在锚定的 5KB 包装脚本里，而在
//! 脚本 `from-import` 解析到的 venv 模块里（swebench `grading.get_eval_report`
//! / `test_spec.test_spec.make_test_spec` / 条件导入的 `constants` 包）——只锚
//! 包装脚本等于真值面裸奔（venv 里 4913 枚在场 pyc 中改写判定真值两模块的
//! `.pyc`，即可在锚定字节零改动下翻转 f2p/p2p）。现产 locator 因此把脚本
//! 实际导入链的六件 `.py` 源逐件 sha256 锚死，与脚本同门同款 fail-loud。
//!
//! 判分子进程执行面三道防线（红队 W3-C34，report 臂 W2-C52 同源纪律）：
//! ① `-B`（解释器不写 `__pycache__`）；② `PYTHONDONTWRITEBYTECODE=1`（①的
//! env 形态，判分脚本派生的嵌套 python 同样禁写）；③ 受控全新空目录作
//! `PYTHONPYCACHEPREFIX`（读取侧重定向——venv 里在场且头部 mtime/size 对得
//! 上的影子 `.pyc` 整体不可达，判分永远纯源执行；report 臂的「判分目录
//! pyc 在场即拒」门对 site-packages 不可行——那是合法安装的既有形态，故
//! 读取侧取重定向而非删除）。缓存目录不清理，留作 tripwire：正常路径下
//! 它应保持全空，出现任何 `.pyc` 即有子进程绕过 ①②。
//!
//! 判定值只从 judge stdout JSON 取（§3.2-3）：本模块只产出原始子进程结果
//! （`Output`），JSON 解读与 note 拼装原样留在 `suite::swev::verify`
//! （af-bench swev.rs:212-257 随迁），Rust 侧不做任何再解释。
//!
//! 判分脚本与 venv 绝对路径绑本机（方案 R2）：判分固定本机执行（容器在
//! anolis、judge 在本机的现行拓扑保持），不在 anolis 跑 judge。

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::ExeclineError;
use crate::suite::swev::sha256_hex;

/// 判分导入链信任锚成员（红队 W3-C34）：判分脚本 from-import 实际解析到的
/// 模块源文件（`.py`）→ sha256。锚的是源，执行面由 [`invoke`] 的禁写 +
/// 读取侧重定向钉在源上（在场 `.pyc` 影子不可达）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportAnchor {
    /// 模块源文件绝对路径。
    pub path: PathBuf,
    /// 源文件 sha256 锚（64 位十六进制小写）。
    pub sha256: String,
}

/// 判分器定位（方案 §3.2-2 / R2：路径参数化，默认 = 本机现产值；
/// P4 起进 SwevUnitSpec 声明，随 unit_version 版本化）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgeLocator {
    /// 判分 venv 的 python 解释器（swebench 4.1 装于其中，swev.rs:205）。
    pub python: PathBuf,
    /// 判分脚本路径（swev.rs:206；字节冻结由指纹门守）。
    pub script: PathBuf,
    /// 判分脚本 sha256 锚（64 位十六进制小写）。
    pub script_sha256: String,
    /// 判分脚本实际导入链的信任锚（红队 W3-C34）。production = swev-judge.py
    /// 三个 from-import 解析到的 swebench 六件源文件。空表 = 自定义 locator
    /// （测试/历史回放）语义「只核对已声明成员」。
    pub import_chain: Vec<ImportAnchor>,
}

impl JudgeLocator {
    /// 本机现产值（← af-bench swev.rs:205-209 硬编码逐字迁移）+ 现产指纹锚
    /// `a292547f…`（方案 §3.1 本机 sha256sum 实测，2026-10-24 复测一致）
    /// + 导入链六件锚（红队 W3-C34，2026-10-27 sha256sum 实测）。
    pub fn production() -> Self {
        let sp = PathBuf::from("/home/pan-ding/swev-venv/lib/python3.14/site-packages");
        Self {
            python: PathBuf::from("/home/pan-ding/swev-venv/bin/python"),
            script: PathBuf::from("/home/pan-ding/workspace/agent黑灯工厂/tools/swev-judge.py"),
            script_sha256: "a292547f3bfce825b0b44997fd5cc145a5aa7ee89d8533729716c0275950617c"
                .into(),
            import_chain: vec![
                // swev-judge.py 顶部两个 from-import 的包链 + 判定真值两模块
                // （grading.get_eval_report / test_spec.test_spec.make_test_spec）
                // + 脚本条件导入的 constants 包（env yml 合成）。包链成员
                // 的 __init__.py 决定 from-import 的解析语义，同在锚内。
                ImportAnchor {
                    path: sp.join("swebench/__init__.py"),
                    sha256: "00776d61343153484ad2d5a43c2ce13202d7e4603de069ee2b112dc39390b862"
                        .into(),
                },
                ImportAnchor {
                    path: sp.join("swebench/harness/__init__.py"),
                    sha256: "6da23bf2681c43d063950f670923484df1c13631e444d9e3e7e8d20445165545"
                        .into(),
                },
                ImportAnchor {
                    path: sp.join("swebench/harness/grading.py"),
                    sha256: "88fc500ebaf692a53457149cec24dcf82dfa2d48f98021a894a087d275f61df4"
                        .into(),
                },
                ImportAnchor {
                    path: sp.join("swebench/harness/constants/__init__.py"),
                    sha256: "c12fe2671fd8b7d8af8f5c711fceb2ca684254e2c1b4cde448422a44b8d04e35"
                        .into(),
                },
                ImportAnchor {
                    path: sp.join("swebench/harness/test_spec/__init__.py"),
                    sha256: "19fe677f658b84984b086a11d037e54932865e72cb579b3196f74ca21218337e"
                        .into(),
                },
                ImportAnchor {
                    path: sp.join("swebench/harness/test_spec/test_spec.py"),
                    sha256: "f9ab368091b5770d2ab6e842e7d56161f87e70a20519576731c122e310820f4a"
                        .into(),
                },
            ],
        }
    }

    /// 指纹门（方案 §3.2-2 fail-loud）：`sha256(script)` 必须等于声明锚，否则
    /// EX3002 拒跑；脚本读不到 = 无法验真，同样拒跑。不降级、不跳判、不静默
    /// 换脚本。导入链成员（红队 W3-C34）同门同款：每件 `.py` 源逐一对锚，
    /// 改一字节/缺席即 EX3002。调用方：`suite::swev::verify` 入口（P3）与 P4
    /// 单元装载（双重防线）。
    pub fn check_fingerprint(&self) -> Result<(), ExeclineError> {
        let script_actual = sha256_hex(&self.script);
        match script_actual {
            Some(actual) if actual == self.script_sha256 => {}
            Some(actual) => {
                return Err(ExeclineError::JudgeFingerprintMismatch {
                    script: self.script.display().to_string(),
                    expected: self.script_sha256.clone(),
                    actual,
                });
            }
            None => {
                return Err(ExeclineError::JudgeFingerprintMismatch {
                    script: self.script.display().to_string(),
                    expected: self.script_sha256.clone(),
                    actual: "脚本不可读（无法验真）".into(),
                });
            }
        }
        for anchor in &self.import_chain {
            match sha256_hex(&anchor.path) {
                Some(actual) if actual == anchor.sha256 => {}
                Some(actual) => {
                    return Err(ExeclineError::JudgeFingerprintMismatch {
                        script: anchor.path.display().to_string(),
                        expected: anchor.sha256.clone(),
                        actual,
                    });
                }
                None => {
                    return Err(ExeclineError::JudgeFingerprintMismatch {
                        script: anchor.path.display().to_string(),
                        expected: anchor.sha256.clone(),
                        actual: "导入链成员不可读（无法验真）".into(),
                    });
                }
            }
        }
        Ok(())
    }
}

/// 判分子进程调用（方案 §3.2-1 唯一调用点；← af-bench swev.rs:205-210 原样，
/// 硬编码路径换 locator 字段）。argv = `[python, -B, script, task_json, log,
/// diff]`——判分脚本恰收到三个位置参数（-B 为解释器旗标，红队 W3-C34）；
/// env 注入仅限两道判分执行面防线（`PYTHONDONTWRITEBYTECODE=1`、受控空
/// `PYTHONPYCACHEPREFIX`，见模块头），其余原样继承、不经 wrapper shell。
/// stdout/stderr/退出码原样交调用方解读（`suite::swev::verify`）。
pub fn invoke(
    locator: &JudgeLocator,
    task_json: &Path,
    log: &Path,
    diff: &Path,
) -> std::io::Result<std::process::Output> {
    // 受控全新空目录：pid + 纳秒唯一，判分并发互不串缓存。建成与否不阻断
    // 判分——prefix 指向缺席目录时读取侧同样 miss（纯源执行不变）。
    let mirror = std::env::temp_dir().join(format!(
        "af-judge-pycache-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let _ = std::fs::create_dir_all(&mirror);
    Command::new(&locator.python)
        .arg("-B")
        .arg(&locator.script)
        .arg(task_json)
        .arg(log)
        .arg(diff)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONPYCACHEPREFIX", &mirror)
        .output()
}
