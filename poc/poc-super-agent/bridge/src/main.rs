//! af-super-agent-bridge — Router 最小皮（PoC-D）到 af 内核**现成 API** 的薄门面。
//!
//! Slow Path 六步里内核侧四步全走既有函数，零新语义：
//!   ① workorder-new   = WorkOrder::new（订单入环；Slow Path 只对订单说话的载体）
//!   ② decision-payload= WorkOrder::decision_payload（被签 payload，长度前缀防重排）
//!   ③ seal            = WorkOrder::seal_decision（DecisionArtifact::seal 同源；
//!                       算法仍是 ed25519-placeholder——四层校验第④层占位是登记在案的
//!                       开放风险#1，BOOT-1 换真验签，本桥接口不变）
//!   ④ validate        = DecisionArtifact::validate_against（四层：hash→角色上限→
//!                       决策权限→占位签名；Reviser 签 Approve 在第③层被拒是本 PoC
//!                       「Router 永远 Reviser」的机械证据）
//!   ⑤ admit           = ReleaseGate::admit（Created→Released 唯一写者：本桥**没有**
//!                       任何裸 release 子命令，工单 release 只经投放闸——三禁·release
//!                       唯一写者收口的可 grep 事实）
//!   ⑥（落工件/留档在 Python 皮层，不进桥）
//!
//! 策略不进桥：桥接受显式 --role/--decision 是为负例探针（证明第②/③层会拒），
//! 生产路径由皮层（admit/guard.py）保证 Router 只以 Reviser/Revise 进来。

use std::fs;
use std::process::ExitCode;

use af_release_gate::{GateConfig, ReleaseGate};
use af_workorder::{
    BudgetVoucher, Decision, DecisionArtifact, GraphVersion, LineBinding, Priority, Role,
    RunContext, Sla, WorkOrder,
};

fn usage() -> ! {
    eprintln!(
        "用法:\n\
         \x20 workorder-new --id <order-id> [--graph-version <v>]\n\
         \x20 decision-payload --workorder <wo.json> --subject <s>\n\
         \x20 seal --workorder <wo.json> --subject <s> --actor <a> --role <Reviser|Approver|Arbiter|Reader> \
         --decision <Approve|Reject|Revise|Arbitrate> --key-id <k>\n\
         \x20 validate --workorder <wo.json> --artifact <art.json> --subject <s>\n\
         \x20 admit --workorder <wo.json> --artifact <art.json> --subject <s> \
         [--review-minutes 30] [--wip-upper 400] [--wip-lower 100] [--minute 600]"
    );
    std::process::exit(2);
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn flag_or(args: &[String], name: &str, default: &str) -> String {
    flag(args, name).unwrap_or_else(|| default.to_string())
}

fn load_workorder(args: &[String]) -> WorkOrder {
    let p = flag(args, "--workorder").unwrap_or_else(|| usage());
    let text = fs::read_to_string(&p).unwrap_or_else(|e| {
        eprintln!("工单读取失败 {p}: {e}");
        std::process::exit(1);
    });
    // 文件形态=workorder-new 的输出信封 {"workorder":{...}}；也接受裸工单对象。
    // 解码走内核严格门（decode_work_order_value：身份字段门禁+未知字段拒绝），
    // 不绕线格式纪律。
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| {
        eprintln!("文件不是 JSON {p}: {e}");
        std::process::exit(1);
    });
    let wo_value = value.get("workorder").cloned().unwrap_or(value);
    af_workorder::decode_work_order_value(wo_value).unwrap_or_else(|e| {
        eprintln!("工单反序列化失败 {p}: {e}");
        std::process::exit(1);
    })
}

fn load_json_member(path: &str, key: &str) -> serde_json::Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("读取失败 {path}: {e}");
        std::process::exit(1);
    });
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| {
        eprintln!("文件不是 JSON {path}: {e}");
        std::process::exit(1);
    });
    value.get(key).cloned().unwrap_or(value)
}

fn parse_role(s: &str) -> Role {
    match s {
        "Reviser" => Role::Reviser,
        "Approver" => Role::Approver,
        "Arbiter" => Role::Arbiter,
        "Reader" => Role::Reader,
        _ => usage(),
    }
}

fn parse_decision(s: &str) -> Decision {
    match s {
        "Approve" => Decision::Approve,
        "Reject" => Decision::Reject,
        "Revise" => Decision::Revise,
        "Arbitrate" => Decision::Arbitrate,
        _ => usage(),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().cloned() else {
        usage();
    };
    // 三禁·release 唯一写者：桥面无裸 release 入口（误用即拒，可 grep 验证）。
    if cmd == "release" {
        eprintln!("拒绝：裸 release 不存在——Created→Released 唯一写者是 ReleaseGate::admit");
        return ExitCode::from(2);
    }
    match cmd.as_str() {
        "workorder-new" => {
            let id = flag(&args, "--id").unwrap_or_else(|| usage());
            let gv = flag_or(&args, "--graph-version", "af-order/poc-1");
            let mut wo = WorkOrder::new(id, GraphVersion::new(gv), Sla::new(None, Priority::Normal));
            // 线格式纪律：Created 初始 run_id 为空（detached），严格信封「缺 run_id
            // 拒绝补默认值」→ 序列化后回读会被拒。Slow Path 入环即绑 run
            // （continue_as_new：换 run 不改 stable_id/身份——decision 哈希锚
            // stable_id，本步不改变决策语义）。run id 取确定性来源=订单 id。
            let run = flag_or(&args, "--run-id", &format!("run-{0}", wo.id()));
            wo.continue_as_new(RunContext::new(run));
            // release 门控前置（配方声明，同 af-state/spec-F*.json 的 line/budget 口径）：
            // LineVersionNotBound/BudgetVoucherNotIssued 不满足时 ReleaseGate::admit 的
            // 闸 B（工单 release 门控副本预检）会拒。绑定走内核现成 API
            // （bind_line/attach_budget_voucher）。预算凭证=配方声明占位
            // （真签名凭证面=af-order/voucher.rs，BOOT 链，如实登记不冒充）。
            if let (Some(line_id), Some(line_ver)) =
                (flag(&args, "--line-id"), flag(&args, "--line-version"))
            {
                let _ = wo.bind_line(LineBinding { line_id, line_version: line_ver });
            }
            if let Some(cents) = flag(&args, "--budget-cents") {
                let _ = wo.attach_budget_voucher(BudgetVoucher {
                    voucher_id: format!("voucher-{0}", wo.id()),
                    amount: cents.parse::<u64>().unwrap_or(0),
                    signed: true,
                });
            }
            println!(
                "{}",
                serde_json::json!({"workorder": wo, "stable_id": wo.stable_id().as_str(),
                                    "run_id": wo.run_id(), "state": format!("{:?}", wo.state())})
            );
            ExitCode::SUCCESS
        }
        "decision-payload" => {
            let wo = load_workorder(&args);
            let subject = flag(&args, "--subject").unwrap_or_else(|| usage());
            println!(
                "{}",
                serde_json::json!({"payload": wo.decision_payload(&subject)})
            );
            ExitCode::SUCCESS
        }
        "seal" => {
            let wo = load_workorder(&args);
            let subject = flag(&args, "--subject").unwrap_or_else(|| usage());
            let actor = flag(&args, "--actor").unwrap_or_else(|| usage());
            let role = parse_role(&flag_or(&args, "--role", "Reviser"));
            let decision = parse_decision(&flag_or(&args, "--decision", "Revise"));
            let key_id = flag(&args, "--key-id").unwrap_or_else(|| usage());
            let art = wo.seal_decision(actor, role, decision, &subject, key_id);
            println!("{}", serde_json::json!({"artifact": art}));
            ExitCode::SUCCESS
        }
        "validate" => {
            let wo = load_workorder(&args);
            let subject = flag(&args, "--subject").unwrap_or_else(|| usage());
            let art_p = flag(&args, "--artifact").unwrap_or_else(|| usage());
            let art: DecisionArtifact = serde_json::from_value(load_json_member(&art_p, "artifact"))
                .unwrap_or_else(|e| {
                    eprintln!("工件反序列化失败 {art_p}: {e}");
                    std::process::exit(1);
                });
            let payload = wo.decision_payload(&subject);
            match art.validate_against(&payload) {
                Ok(()) => {
                    println!("{}", serde_json::json!({"ok": true, "layers": 4}));
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    println!(
                        "{}",
                        serde_json::json!({"ok": false, "error": format!("{e}"),
                                           "error_kind": format!("{e:?}")})
                    );
                    ExitCode::FAILURE
                }
            }
        }
        "admit" => {
            // admit 双闸路径（本批实作两道真闸）：
            //   闸A = DecisionArtifact 四层校验（不过=拒绝投放——未经签核的单不放行）；
            //   闸B = ReleaseGate::admit（工单 release 门控副本预检+审阅权重+CONWIP）。
            // 「F 门单 admit 两查（RequiredInputMissing/InputGateNotGreen）」是 PoC-F
            // 职责，本桥如实登记为占位（two_check_placeholder），不冒充已做。
            let mut wo = load_workorder(&args);
            let subject = flag(&args, "--subject").unwrap_or_else(|| usage());
            let art: DecisionArtifact = serde_json::from_value(load_json_member(
                &flag(&args, "--artifact").unwrap_or_else(|| usage()),
                "artifact",
            ))
            .unwrap_or_else(|e| {
                eprintln!("工件反序列化失败: {e}");
                std::process::exit(1);
            });
            let payload = wo.decision_payload(&subject);
            if let Err(e) = art.validate_against(&payload) {
                println!(
                    "{}",
                    serde_json::json!({"admitted": false, "gate": "A-artifact-four-layer",
                                       "error": format!("{e}"),
                                       "two_check_placeholder":
                                       "F-门单 admit 两查（RequiredInputMissing/InputGateNotGreen）=PoC-F 职责，未在本批"})
                );
                return ExitCode::FAILURE;
            }
            let review = flag_or(&args, "--review-minutes", "30").parse::<f64>().unwrap();
            let upper = flag_or(&args, "--wip-upper", "400").parse::<f64>().unwrap();
            let lower = flag_or(&args, "--wip-lower", "100").parse::<f64>().unwrap();
            let minute = flag_or(&args, "--minute", "600").parse::<u32>().unwrap();
            let mut gate = ReleaseGate::new(GateConfig {
                wip_upper_minutes: upper,
                wip_lower_minutes: lower,
                night_window: None,
            })
            .expect("投放闸配置非法");
            match gate.admit(&mut wo, review, minute) {
                Ok(permit) => {
                    // Permit/Ticket/RampPhase 不实现 Serialize（内核面未derive）——
                    // 逐字段投影，不加内核 derive（桥不改内核 crate）。
                    println!(
                        "{}",
                        serde_json::json!({
                            "admitted": true, "gate": "B-release-gate",
                            "state_after": format!("{:?}", wo.state()),
                            "permit": {
                                "order_id": permit.order_id,
                                "ticket": {
                                    "item_id": permit.ticket.item_id,
                                    "config": permit.ticket.config,
                                    "weight": permit.ticket.weight,
                                    "admitted_at": permit.ticket.admitted_at,
                                },
                                "ramp": format!("{:?}", permit.ramp),
                            },
                            "released_workorder": wo,
                            "two_check_placeholder":
                            "F-门单 admit 两查（RequiredInputMissing/InputGateNotGreen）=PoC-F 职责，未在本批"
                        })
                    );
                    ExitCode::SUCCESS
                }
                Err(rej) => {
                    println!(
                        "{}",
                        serde_json::json!({"admitted": false, "gate": "B-release-gate",
                                           "reject": format!("{rej}"),
                                           "two_check_placeholder": "PoC-F 职责，未在本批"})
                    );
                    ExitCode::FAILURE
                }
            }
        }
        _ => usage(),
    }
}
