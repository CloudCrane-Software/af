//! INV-2 判定镜像（A-07..A-10）。
//!
//! 镜像源：af-wt-phase2-dsl@phase2-g2b crates/af-dsl/src/validate/inv2.rs。
//! 抽象：3 节点（全路径索引 0..3）、2 store 槽（S0/S1，「1-2 槽」短域）；
//! 点号路径解析/字符串比较 → 节点索引相等；writes[] → 每节点写目标
//! {无, S0, S1}；writer 悬空 → 哨兵值 DANGLING（对应 inv2.rs:19-22 跳过支）。
//! 嵌套体「含于前缀」→ 布尔 in_body 位（inv2.rs:57-59 前缀过滤的语义投影）。
//!
//! CBMC 工程纪律（FV-0 阻断二教训）：镜像与 harness 前提一律显式 for 循环 +
//! 布尔累加器，不用 iterator 组合器（.find/.filter/.any 在符号执行下病态展开）。
//!
//! 实装抑制（镜像以域值/前提显式化，CONFLICTS.md 有专节）：
//! - 零写者不报 E2311（inv2.rs:5-6 + :42-52 条件不含 len==0）——与 A-07 陈述
//!   字面「基数 ≠ 1」的分歧由 conflict_demo::a07_literal_statement_conflict 实证。
//! - writer 悬空（E2001 已定罪）→ 该 store 整体跳过（inv2.rs:19-22）。
//! - carried 只收已声明 store（inv2.rs:63）——镜像域内两 store 恒已声明。

use crate::{C_E2311, C_E2312, C_E2211};

pub const DANGLING: u8 = 3; // writer 路径解析失败哨兵（inv2.rs:20 else continue）

/// 写者闭包镜像输入（inv2.rs:16-53 输入投影；2 store 联合域）。
#[derive(Clone, Copy)]
pub struct Inv2Mirror {
    /// 每节点写目标：0=无、1=S0、2=S1（inv2.rs:26 `writes.iter().any(|w| w == name)`）。
    pub writes: [u8; 3],
    /// NodeRef.in_map（inv2.rs:30）。
    pub in_map: [bool; 3],
    /// 每 store 已解析声明 writer 节点 0..3；DANGLING=跳过支（inv2.rs:19-22）。
    pub declared: [u8; 2],
}

/// 单 store 的实际写者扫描（inv2.rs:23-27 的显式循环重写）。
/// 返回 (基数, 唯一写者索引；基数≠1 时无定义)。
fn actual_writers(m: &Inv2Mirror, s: usize) -> (usize, usize) {
    let target = s as u8 + 1;
    let mut cnt = 0usize;
    let mut uniq = 0usize;
    for n in 0..3 {
        if m.writes[n] == target {
            cnt += 1;
            uniq = n;
        }
    }
    (cnt, uniq)
}

/// 写者闭包核对镜像 = inv2.rs:16-53（① E2311 + ② E2312；③ carried 另函数）。
pub fn inv2_writer_check(m: &Inv2Mirror) -> u32 {
    let mut out = 0;
    for s in 0..2 {
        if m.declared[s] == DANGLING {
            continue; // inv2.rs:19-22：写者路径悬空已由 refs 层 E2001 报，跳过
        }
        // ② map 体节点写共享槽（inv2.rs:28-40；与 ① 独立判定）+ ① 实际写者集。
        let (cnt, uniq) = actual_writers(m, s);
        for n in 0..3 {
            // inv2.rs:29-40：actual 内每个 in_map 节点一条 E2312。
            if m.writes[n] == s as u8 + 1 && m.in_map[n] {
                out |= C_E2312;
            }
        }
        // ① 写者集合势 = 1 且等于声明值（inv2.rs:41-52）。
        // 注意：cnt == 0 不报（inv2.rs:5-6 模块注记「零写者不报」）。
        let multi = cnt > 1;
        let single_mismatch = cnt == 1 && uniq != m.declared[s] as usize;
        if multi || single_mismatch {
            out |= C_E2311;
        }
    }
    out
}

/// carried 覆盖镜像输入（inv2.rs:54-87 输入投影）。
#[derive(Clone, Copy)]
pub struct CarriedMirror {
    /// 节点全路径是否带 "{owner}." 前缀（inv2.rs:57-59；嵌套体天然含于前缀）。
    pub in_body: [bool; 3],
    /// 每节点写目标 {无, S0, S1}（inv2.rs:62-66）。
    pub writes: [u8; 3],
    /// carried[] 是否登记 S0/S1（inv2.rs:68-74）。
    pub carried: [bool; 2],
}

/// 单 store 的「体内写入」判定（inv2.rs:56-67 的显式循环重写）。
fn written_inside(m: &CarriedMirror, s: usize) -> bool {
    let target = s as u8 + 1;
    let mut hit = false;
    for n in 0..3 {
        // inv2.rs:58-60：路径前缀过滤（体外节点不计；嵌套体含于前缀）。
        // inv2.rs:62-66（:63 只收已声明 store——镜像域内两 store 恒已声明）。
        if m.in_body[n] && m.writes[n] == target {
            hit = true;
        }
    }
    hit
}

/// carried 覆盖核对镜像 = inv2.rs:54-87（③ E2211）。
pub fn carried_check(m: &CarriedMirror) -> u32 {
    let mut out = 0;
    for s in 0..2 {
        // inv2.rs:75-86：体外声明的 store 被体内写而 carried 未登记 ⇒ E2211。
        if written_inside(m, s) && !m.carried[s] {
            out |= C_E2211;
        }
    }
    out
}

// ==================== A-07 · INV-2 soundness：写者集合恰一 ====================
// 陈述：某 store 写者集合基数 ≠ 1 或 ≠ 声明 writer ⇒ E2311。
// 实装域注记：len == 0 不报（inv2.rs:5-6，冲突见 CONFLICTS.md #2）；writer
// 悬空跳过（inv2.rs:19-22）。harness 前提 = 声明已解析 ∧ 至少一实际写者。

#[kani::proof]
fn a07_soundness() {
    let mut m = Inv2Mirror {
        writes: [0; 3],
        in_map: [false; 3],
        declared: [0; 2],
    };
    for n in 0..3 {
        m.writes[n] = kani::any();
        kani::assume(m.writes[n] < 3);
        m.in_map[n] = kani::any();
    }
    for s in 0..2 {
        m.declared[s] = kani::any();
        kani::assume(m.declared[s] < 3); // 已解析域（悬空 = 跳过支，域外注记）
    }
    let out = inv2_writer_check(&m);
    for s in 0..2 {
        let (cnt, uniq) = actual_writers(&m, s);
        // 实装域内前提：至少一写者（cnt==0 角落 → conflict_demo 实证）。
        kani::assume(cnt >= 1);
        // 陈述违例谓词：基数 ≠ 1 或（基数=1 时）≠ 声明 writer。
        let mismatch = cnt != 1 || uniq != m.declared[s] as usize;
        assert!(
            !mismatch || out & C_E2311 != 0,
            "A-07 soundness: 写者集合 ≠1 或 ≠声明（cnt≥1）必报 E2311"
        );
    }

    // 非空洞性 sanity：双写者必报。
    let bad = Inv2Mirror {
        writes: [0, 1, 1], // S0 双写者（N1、N2）
        in_map: [false, false, false],
        declared: [0, DANGLING],
    };
    assert!(inv2_writer_check(&bad) & C_E2311 != 0, "sanity: 双写者触发 E2311");
    // 单写者但 ≠ 声明。
    let wrong = Inv2Mirror {
        writes: [0, 1, 0],
        in_map: [false, false, false],
        declared: [0, DANGLING],
    };
    assert!(inv2_writer_check(&wrong) & C_E2311 != 0, "sanity: 写者≠声明触发 E2311");
}

#[kani::proof]
fn a07_completeness() {
    let mut m = Inv2Mirror {
        writes: [0; 3],
        in_map: [false; 3],
        declared: [0; 2],
    };
    for n in 0..3 {
        m.writes[n] = kani::any();
        kani::assume(m.writes[n] < 3);
        m.in_map[n] = kani::any();
    }
    for s in 0..2 {
        m.declared[s] = kani::any();
        kani::assume(m.declared[s] < 3);
    }
    let out = inv2_writer_check(&m);
    // 合法域（每 store 恰一写者且 = 声明）⇒ 不因 E2311 拒。
    let mut legal = true;
    for s in 0..2 {
        let (cnt, uniq) = actual_writers(&m, s);
        if cnt != 1 || uniq != m.declared[s] as usize {
            legal = false;
        }
    }
    assert!(!legal || out & C_E2311 == 0, "A-07 completeness: 恰一写者且=声明 ⇒ 无 E2311");

    // 具体合法角点。
    let ok = Inv2Mirror {
        writes: [1, 2, 0],
        in_map: [false, false, false],
        declared: [0, 1],
    };
    assert!(inv2_writer_check(&ok) & C_E2311 == 0, "sanity: 合法角点无 E2311");
}

// ==================== A-08 · INV-2 soundness：map 体禁写体外 store ====================
// 陈述：map 体节点写体外声明 store ⇒ E2312（N 实例 = N 写者）。
// 悬空跳过支同 A-07（域外注记）；E2312 与写者势独立（inv2.rs:28-40 注释）。

#[kani::proof]
fn a08_soundness() {
    let mut m = Inv2Mirror {
        writes: [0; 3],
        in_map: [false; 3],
        declared: [0; 2],
    };
    for n in 0..3 {
        m.writes[n] = kani::any();
        kani::assume(m.writes[n] < 3);
        m.in_map[n] = kani::any();
    }
    for s in 0..2 {
        m.declared[s] = kani::any();
        kani::assume(m.declared[s] < 3); // 已解析域
    }
    let out = inv2_writer_check(&m);
    let mut viol = false;
    for s in 0..2 {
        for n in 0..3 {
            if m.writes[n] == s as u8 + 1 && m.in_map[n] {
                viol = true;
            }
        }
    }
    assert!(!viol || out & C_E2312 != 0, "A-08 soundness: map 体写体外 store 必报 E2312");
    // 非 map 体写者不产 E2312（码不串位）。
    let mut no_map_write = true;
    for s in 0..2 {
        for n in 0..3 {
            if m.writes[n] == s as u8 + 1 && m.in_map[n] {
                no_map_write = false;
            }
        }
    }
    assert!(!no_map_write || out & C_E2312 == 0);

    // 非空洞性 sanity：map 体单写者（势恰一、= 声明）仍必报 E2312——独立于 ①。
    let solo = Inv2Mirror {
        writes: [1, 0, 0],
        in_map: [true, false, false],
        declared: [0, DANGLING],
    };
    let o = inv2_writer_check(&solo);
    assert!(o & C_E2312 != 0, "sanity: map 体写 S0 触发 E2312");
    assert!(o & C_E2311 == 0, "sanity: 该形态势恰一且=声明，无 E2311（两码独立）");
}

#[kani::proof]
fn a08_completeness() {
    let mut m = Inv2Mirror {
        writes: [0; 3],
        in_map: [false; 3],
        declared: [0; 2],
    };
    for n in 0..3 {
        m.writes[n] = kani::any();
        kani::assume(m.writes[n] < 3);
        m.in_map[n] = kani::any();
    }
    for s in 0..2 {
        m.declared[s] = kani::any();
        kani::assume(m.declared[s] < 3);
    }
    let out = inv2_writer_check(&m);
    // 无 map 体写者 ⇒ 不因 E2312 拒。
    let mut has_map_writer = false;
    for n in 0..3 {
        if m.in_map[n] && m.writes[n] != 0 {
            has_map_writer = true;
        }
    }
    assert!(
        has_map_writer || out & C_E2312 == 0,
        "A-08 completeness: 无 map 体写者 ⇒ 无 E2312"
    );
    let ok = Inv2Mirror {
        writes: [1, 2, 0],
        in_map: [false, false, false],
        declared: [0, 1],
    };
    assert!(inv2_writer_check(&ok) & C_E2312 == 0, "sanity: 无 map 写角点无 E2312");
}

// ==================== A-09 · INV-2 soundness：carried 覆盖（含反向 completeness） ====================
// 陈述：carried[] 未覆盖「体内写入的体外 store」⇒ E2211；覆盖通过 ⇒ 不因 E2211 拒。

#[kani::proof]
fn a09_soundness() {
    let mut m = CarriedMirror {
        in_body: [false; 3],
        writes: [0; 3],
        carried: [false; 2],
    };
    for n in 0..3 {
        m.in_body[n] = kani::any();
        m.writes[n] = kani::any();
        kani::assume(m.writes[n] < 3);
    }
    for s in 0..2 {
        m.carried[s] = kani::any();
    }
    let out = carried_check(&m);
    let mut viol = false;
    for s in 0..2 {
        if written_inside(&m, s) && !m.carried[s] {
            viol = true;
        }
    }
    assert!(!viol || out & C_E2211 != 0, "A-09 soundness: carried 未覆盖必报 E2211");
    // 码不串位：全覆盖 ⇒ 无 E2211。
    assert!(viol || out & C_E2211 == 0);

    // 非空洞性 sanity：体外节点写 S0 不计（前缀过滤），体内写 S1 未登记 ⇒ E2211。
    let tricky = CarriedMirror {
        in_body: [false, true, false],
        writes: [1, 2, 1], // n0 体外写 S0（不计）；n1 体内写 S1
        carried: [false, false], // S1 未登记
    };
    assert!(carried_check(&tricky) & C_E2211 != 0, "sanity: 体内写未登记触发 E2211");
    // 且体外写不触发（前缀过滤镜像生效）：S0 未登记但只有体外写者。
    let outside_only = CarriedMirror {
        in_body: [false, false, false],
        writes: [1, 0, 0],
        carried: [false, false],
    };
    assert!(carried_check(&outside_only) == 0, "sanity: 纯体外写不触发 E2211");
}

#[kani::proof]
fn a09_completeness() {
    let mut m = CarriedMirror {
        in_body: [false; 3],
        writes: [0; 3],
        carried: [false; 2],
    };
    for n in 0..3 {
        m.in_body[n] = kani::any();
        m.writes[n] = kani::any();
        kani::assume(m.writes[n] < 3);
    }
    for s in 0..2 {
        m.carried[s] = kani::any();
    }
    let out = carried_check(&m);
    // 反向（陈述句末）：覆盖核对通过 ⇒ 不因 E2211 拒。
    let mut covered = true;
    for s in 0..2 {
        if written_inside(&m, s) && !m.carried[s] {
            covered = false;
        }
    }
    assert!(!covered || out & C_E2211 == 0, "A-09 completeness: 覆盖 ⇒ 无 E2211");
}

// ==================== A-10 · INV-2 completeness + 性质引理 ====================
// ① 写者闭包核对通过 ⇒ 每 store 写者集合恰为声明 writer 单点，不因 E2311/E2312 拒。
// ② 性质引理（Rocq 主证、Kani 静态面）：静态单写者 + 执行面按 writer 路由
//   （TCB，【矩阵】:49 显式前提）⇒ 任意 store 无并发写冲突。

#[kani::proof]
fn a10_completeness_closure() {
    let mut m = Inv2Mirror {
        writes: [0; 3],
        in_map: [false; 3],
        declared: [0; 2],
    };
    for n in 0..3 {
        m.writes[n] = kani::any();
        kani::assume(m.writes[n] < 3);
        m.in_map[n] = kani::any();
    }
    for s in 0..2 {
        m.declared[s] = kani::any();
        kani::assume(m.declared[s] < 3);
    }
    let out = inv2_writer_check(&m);
    // 前提：写者闭包核对通过（无 E2311/E2312）∧ 每 store 至少一实际写者。
    // 域注记：零写者角落实装不报码（inv2.rs:5-6——CONFLICTS.md #2，A-07 冲突
    // 同根经 ① 复现：零写者时「核对通过」但写者集合 ≠ 声明单点），故 ① 的
    // 结论域 = 至少一写者的 store；此前提如实记录，不改陈述。
    let mut passed = out & (C_E2311 | C_E2312) == 0;
    for s in 0..2 {
        let (cnt, _) = actual_writers(&m, s);
        if cnt == 0 {
            passed = false;
        }
    }
    // 结论：每 store 写者集合恰为声明 writer 单点（且非 map 体写者）。
    let mut exact = true;
    for s in 0..2 {
        let (cnt, uniq) = actual_writers(&m, s);
        if cnt != 1 || uniq != m.declared[s] as usize || m.in_map[uniq] {
            exact = false;
        }
    }
    assert!(!passed || exact, "A-10①: 核对通过（且 ≥1 写者）⇒ 写者集合恰为声明单点且无 map 写");
}

#[kani::proof]
fn a10_no_conflict_lemma() {
    // 抽象写事件模型（Rocq 引理的 Kani 静态面对应物）：
    // 静态面（a10_completeness_closure 已证）：writer 闭包通过 ⇒ 每 store 的
    // 合法执行者集合 = {w*}。
    // 机制前提（TCB，【矩阵】:49 显式登记）：执行面按 writer 路由——写请求的
    // 【生效写者】恒为路由目标 w*，与请求方自称身份（claimed）无关。
    let mut w_star: u8 = kani::any();
    kani::assume(w_star < 3);
    let mut claimed = [0u8; 2]; // 两个并发写请求的自称身份（域 3×3 全枚举）
    for c in claimed.iter_mut() {
        *c = kani::any();
        kani::assume(*c < 3);
    }
    // 机制镜像：route_by_writer(c) = w*（单写者静态事实 + 按 writer 路由）。
    let routed = [w_star, w_star];
    // 并发写冲突 ⇔ 同 store 两个并发生效写者不同。
    assert!(
        routed[0] == routed[1],
        "A-10②: 静态单写者 + 按 writer 路由（TCB 前提）⇒ 无并发写冲突"
    );
    // 非空洞性对照（反事实，具体值）：若无机制前提而按自称路由，则自称
    // (0,1) 的两请求即为两个不同写者——冲突谓词在域内非退化，义务确由
    // TCB 前提兑现（前提承载全部内容，【矩阵】:49 归属如实）。
    let by_claim = [0u8, 1u8];
    assert!(by_claim[0] != by_claim[1], "对照: 按自称路由时冲突可发生");
}
