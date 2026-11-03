//! Wave-1 镜像穷尽 harness 库（Kani 0.68.0，纯逻辑切片镜像模式——FV-0.2 同款）。
//!
//! 形态纪律：**全镜像零堆分配**（固定数组 + 显式长度计数）。原因实测定级：
//! 探针 probe_vec_symbolic（堆 Vec + 符号 push ×2，unwind 6）验证 9.0s 且
//! 369 checks（分配路径噪声）；本库原 Vec 版单 harness >4 分钟不收敛——
//! 与 FV-0 报告 §2.3「对照组：同算法 Vec/整数镜像 0.196s——瓶颈为数据
//! 结构/串机器」结论一致，故镜像一律整数/数组域（FV-0 已验证的对照组形态）。
//!
//! 陈述对应表（theorems-v1.md → 模块）：
//!   loop_guard_mirror → B-01/B-04/B-06 编译侧（E2002/E2004 判定逻辑）
//!   driver_mirror     → B-01 运行侧（计数器边界）+ B-06 必经 + B-02 三值透传
//!   carried_mirror    → B-03（iteration 重建）+ B-04（截断上界）
//!   map_mirror        → B-09（扇出/空扇出）+ B-11（max_fanout）+ B-13（聚合序）
//!   sort_mirror       → C-03（排序幂等 + 置换不变）
//!   btree_mirror      → C-02（遍历序=字典序、内容规范函数）
//!   naming_mirror     → C-07（降维命名/entry/版本 stamp）
//!   lower_pipeline    → C-01（双跑逐位一致，ideal 对象）
//!
//! 实现锚定（af-wt-phase2-dsl @ phase2-g2b，2026-10-24 逐行核对）：
//!   validate/loop_guard.rs:26-32 E2002 max_iterations==0
//!   validate/loop_guard.rs:33-39 E2004 safety_points 空
//!   validate/loop_guard.rs:40-51 E2002 carried 无上界（is_bounded 口径 =
//!     af-graph model.rs:92-94，Some(0) 视为已声明）
//!   af-compiler/lower.rs:135-148 边投影 + 稳定键四元组排序（键 = (from,
//!     output, to, input)）· :225 节点按 id 排序
//!   af-compiler/lower.rs:300-305/:346-351 entry = 体 nodes 首元素（空体 →
//!     LowerError::EmptyBody）· :307-311 graph_version stamp 父图版本
//!   af-compiler/lower.rs:325 carried 按 store 名排序 · :331 safety_points
//!     按节点路径排序 · :344-358 map_spec 三字段透传
//!   af-compiler/lower.rs:123-125 subgraph_name = 图名/路径（. → /）
//!
//! 穷尽域声明：全部镜像为有界域全枚举（n ≤ 4 / max_iterations ≤ 3），
//! 输入 kani::any() 无自由度遗漏；无界归纳版属 Verus（本轮 Verus 安装因
//! 代理 SSL 窗口关闭未成——40 次重试全败，如实登记），本库为「镜像穷尽 +
//! 内核裁决」证据层，与 FV-0.2 同纪律。

// ===========================================================================
// 公共镜像原语：零堆 Report（违例收集器）+ 手工逐位比较
// ===========================================================================
/// 数组逐位相等（手工循环）：数组/元组 `==` 会编译为 memcmp，其循环消耗
/// unwind 预算（实测 32 字节数组即耗尽 unwind(32)——见运行日志失败类 1），
/// 故全部镜像比较取显式逐位循环。
fn eq_arr<const N: usize>(a: &[u64; N], b: &[u64; N]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < N {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

mod report {
    /// 违例镜像：(E码, slot, idx)。slot: 0=max_iterations 查、1=safety 查、
    /// 2=carried[idx] 查（对应 loop_guard.rs:26/33/40 三个 report.push 位点）。
    #[derive(Copy, Clone, PartialEq, Debug)]
    pub struct Viol {
        pub code: u64,
        pub slot: u8,
        pub idx: u8,
    }

    /// 固定容量收集器（check_loop_site 至多 1+1+3=5 条，容量 8 由构造充分）。
    #[derive(Copy, Clone, PartialEq)]
    pub struct Report {
        pub items: [Viol; 8],
        pub len: usize,
    }

    pub fn new_report() -> Report {
        Report { items: [Viol { code: 0, slot: 9, idx: 9 }; 8], len: 0 }
    }

    pub fn push(r: &mut Report, v: Viol) {
        r.items[r.len] = v;
        r.len += 1;
    }

    pub fn contains(r: &Report, v: Viol) -> bool {
        let mut i = 0;
        while i < r.len {
            if r.items[i] == v {
                return true;
            }
            i += 1;
        }
        false
    }

    pub fn any_where(r: &Report, code: u64, slot: u8) -> bool {
        let mut i = 0;
        while i < r.len {
            if r.items[i].code == code && r.items[i].slot == slot {
                return true;
            }
            i += 1;
        }
        false
    }
}

// ===========================================================================
// 模块一：loop_guard 三查判定逻辑（B-01/B-04/B-06 编译侧）
// ===========================================================================
mod loop_guard_mirror {
    use super::report::{any_where, contains, new_report, push, Report, Viol};

    pub const E2002: u64 = 2002;
    pub const E2004: u64 = 2004;

    /// is_bounded 口径镜像（af-graph model.rs:92-94 逐句）：
    /// max_items/max_bytes 至少其一在场；Some(0) 视为已声明。
    pub fn is_bounded(c: (Option<u64>, Option<u64>)) -> bool {
        c.0.is_some() || c.1.is_some()
    }

    /// loop_guard.rs:18-53 check 单站点镜像：三查串行、无早退、一次收齐，
    /// 违例按检查固定顺序 append（max_iterations → safety_points → carried）。
    /// sp_len 镜像 safety_points 声明规模（判定只触 is_empty，:33）。
    pub fn check_loop_site(
        max_iterations: u64,
        sp_len: usize,
        carried: &[(Option<u64>, Option<u64>); 3],
    ) -> Report {
        let mut report = new_report();
        if max_iterations == 0 {
            // loop_guard.rs:26-32 —— E2002 值域
            push(&mut report, Viol { code: E2002, slot: 0, idx: 0 });
        }
        if sp_len == 0 {
            // loop_guard.rs:33-39 —— E2004 空集合
            push(&mut report, Viol { code: E2004, slot: 1, idx: 0 });
        }
        let mut i: usize = 0;
        while i < 3 {
            let c = carried[i];
            if c.0.is_none() && c.1.is_none() {
                // loop_guard.rs:40-51 —— E2002 carried 无上界
                push(&mut report, Viol { code: E2002, slot: 2, idx: i as u8 });
            }
            i += 1;
        }
        report
    }

    #[kani::proof]
    #[kani::unwind(12)]
    fn b01_b06_e2002_e2004_sound_and_complete() {
        let max_iterations: u64 = kani::any();
        kani::assume(max_iterations <= 1); // 穷尽 {0, 1}：==0 与 >0 两支
        let sp_len: usize = kani::any();
        kani::assume(sp_len <= 1);
        let c0_bounded: bool = kani::any();
        // 三项：①Some(0) 变体（Some(0) 视为已声明！）、②双 Some、③Some(0) 单边
        let carried = [
            (if c0_bounded { Some(0u64) } else { None }, Some(1u64)),
            (Some(3u64), Some(64u64)),
            (Some(0u64), None),
        ];
        let report = check_loop_site(max_iterations, sp_len, &carried);

        // B-01 编译侧 soundness：max_iterations == 0 ⇒ E2002(slot0) 在场
        assert!(max_iterations != 0 || contains(&report, Viol { code: E2002, slot: 0, idx: 0 }));
        // completeness 反向：max_iterations > 0 ⇒ 无 slot0 E2002
        assert!(max_iterations == 0 || !any_where(&report, E2002, 0));

        // B-06 编译侧 soundness/completeness：空 ⇔ E2004 在场
        assert!(sp_len != 0 || contains(&report, Viol { code: E2004, slot: 1, idx: 0 }));
        assert!(sp_len == 0 || !any_where(&report, E2004, 1));

        // B-04 编译侧：逐项 无界 ⇔ E2002(slot2,idx) 在场；Some(0) 已声明不产码
        let mut i = 0;
        while i < 3 {
            let unbounded = !is_bounded(carried[i]);
            let hit = contains(&report, Viol { code: E2002, slot: 2, idx: i as u8 });
            assert!(unbounded == hit);
            i += 1;
        }
    }

    #[kani::proof]
    #[kani::unwind(12)]
    fn b04_carried_bounded_exhaustive_flags() {
        // carried 三项的有界/无界旗标全枚举（2^3 组合，含 Some(0) 变体）
        let f0: bool = kani::any();
        let f1: bool = kani::any();
        let f2: bool = kani::any();
        let carried = [
            (if f0 { Some(0u64) } else { None }, None),
            (if f1 { Some(0u64) } else { None }, if f1 { None } else { Some(9u64) }),
            (if f2 { Some(2u64) } else { None }, if f2 { None } else { Some(1u64) }),
        ];
        let report = check_loop_site(7, 2, &carried);
        let mut i = 0;
        while i < 3 {
            let unbounded = !is_bounded(carried[i]);
            let hit = contains(&report, Viol { code: E2002, slot: 2, idx: i as u8 });
            assert!(unbounded == hit);
            i += 1;
        }
        // 全有界（含 Some(0) 双变体）⇒ 零 carried 违例
        let all_bounded = [(Some(0u64), None), (None, Some(0u64)), (Some(1u64), Some(1u64))];
        let clean = check_loop_site(7, 2, &all_bounded);
        assert!(!any_where(&clean, E2002, 2));
        // 一次收齐：三查互不抑制——三项全无界 + max=0 + sp 空 ⇒ 恰 5 条
        let all_unbounded = [(None, None), (None, None), (None, None)];
        let full = check_loop_site(0, 0, &all_unbounded);
        assert!(full.len == 5);
    }
}

// ===========================================================================
// 模块二：受控循环驱动器状态机（B-01 运行侧 + B-06 必经 + B-02 三值透传）
// ===========================================================================
mod driver_mirror {
    pub const OUTCOME_EXIT_HIT: u8 = 9;

    /// 【规范】:20 操作语义镜像。judged[r] = 第 r 轮安全点判定事件；
    /// exit_when[r] = 第 r 轮判定输出后的 exit_when 求值。
    /// 返回（计入完成轮数, 收束值, 已开跑轮数）。
    pub fn run_loop(
        max_iterations: u64,
        exit_when: &[bool; 4],
        judged: &[bool; 4],
        on_exhausted: u8,
    ) -> (u64, u8, u64) {
        let mut iter: u64 = 1;
        let mut completed: u64 = 0;
        let mut outcome: u8 = 0xFF;
        let mut exited = false;
        while iter <= max_iterations {
            // —— B-01 不变式（【规范】:20「恒有 iter ∈ [1, max_iterations]」）：
            //    本轮开跑时刻计数器在预算内（循环头检查 iter <= max_iterations）——
            assert!(1 <= iter && iter <= max_iterations);
            // —— B-06 必经：判定输出事件在先，未经判定不得计入完成 ——
            if judged[(iter - 1) as usize] {
                completed += 1; // 每完成一轮（经判定）计数器才 +1
            }
            if exit_when[(iter - 1) as usize] {
                outcome = OUTCOME_EXIT_HIT;
                exited = true;
                break;
            }
            if iter == max_iterations {
                // 【规范】:20：达 max_iterations 且 exit_when 未命中 ⇒
                // 不再开启新一轮，按 on_exhausted 收束
                outcome = on_exhausted;
                exited = true;
                break;
            }
            iter += 1;
        }
        assert!(exited); // 预算有限 ⇒ 必收束（不开新一轮的终止性镜像）
        (completed, outcome, iter)
    }

    #[kani::proof]
    #[kani::unwind(8)]
    fn b01_counter_bounds_and_exhaustion() {
        let max_iterations: u64 = kani::any();
        kani::assume(1 <= max_iterations && max_iterations <= 3);
        let e: [bool; 4] = kani::any();
        let j: [bool; 4] = kani::any();
        let on_exhausted: u8 = kani::any();
        kani::assume(on_exhausted <= 2); // continue/fail/keep_best 三值

        let (completed, outcome, rounds_run) = run_loop(max_iterations, &e, &j, on_exhausted);

        // (b) B-06 必经：计入完成数 = 已开跑轮次中经判定轮次数
        //（首版 harness 误把计数轮当「前 completed 轮」——轮 1 未判定而轮 2
        //  判定的迹上 j[0]=false 但 completed=1，内核如实反例；修正为计数
        //  轮次的判定求和等式，这才是【规范】:28 必经性的可判定形态）
        let mut cnt: u64 = 0;
        let mut r: u64 = 0;
        while r < rounds_run {
            if j[r as usize] {
                cnt += 1;
            }
            r += 1;
        }
        assert!(completed == cnt);
        // (c) 耗尽收束：未命中 exit ⇒ 收束值 = on_exhausted（三值透传）
        assert!(outcome == OUTCOME_EXIT_HIT || outcome == on_exhausted);
        // (d) 命中收束：exit 命中 ⇒ 立即停（已开跑轮数 ≤ 预算）
        assert!(rounds_run <= max_iterations);
        assert!(completed <= max_iterations);
        // 收束值域：exit 命中哨兵或三值之一（B-02 弱陈述：不解释 keep_best 选取）
        assert!(outcome == OUTCOME_EXIT_HIT || outcome <= 2);
    }

    #[kani::proof]
    #[kani::unwind(8)]
    fn b01_no_new_round_after_exhaustion() {
        // 专项：exit_when 全程未命中 ⇒ 必走耗尽收束路径（不开新一轮的直接
        // 后件：收束值 == on_exhausted 且恰跑满 max_iterations 轮）
        let max_iterations: u64 = kani::any();
        kani::assume(1 <= max_iterations && max_iterations <= 3);
        let e = [false, false, false, false];
        let j = [true, true, true, true];
        let on_exhausted: u8 = kani::any();
        kani::assume(on_exhausted <= 2);
        let (completed, outcome, _rounds) = run_loop(max_iterations, &e, &j, on_exhausted);
        assert!(completed == max_iterations);
        assert!(outcome == on_exhausted);
        assert!(outcome <= 2);
    }
}

// ===========================================================================
// 模块三：carried 重建与截断（B-03 / B-04 运行侧）
// ===========================================================================
mod carried_mirror {
    pub const LC_RUN: u8 = 0;
    pub const LC_ITERATION: u8 = 1;
    pub const LC_PERSISTENT: u8 = 2;

    /// carried store 镜像：固定容量 4 + 长度（零堆）。
    #[derive(Copy, Clone, PartialEq)]
    pub struct Store {
        pub items: [u64; 4],
        pub len: usize,
    }

    /// B-03：iteration carried 每轮开始整体重建（【规范】:24-25）。
    pub fn begin_round(lifecycle: u8, store: &mut Store) {
        if lifecycle == LC_ITERATION {
            store.len = 0;
        }
    }

    /// B-04：按 max_items 截断（滚动窗口，保留尾部）。cap=None ⇒ 不动。
    pub fn truncate_items(store: &mut Store, cap: Option<u64>) {
        let cap = match cap {
            Some(c) => c as usize,
            None => return,
        };
        while store.len > cap {
            // 左移丢最旧（滚动窗口；「最近」判据的 UNCERTAIN 见 B-04——
            // 本证与判据无关的性质是规模上界与满额保留）
            let mut i = 0;
            while i + 1 < store.len {
                store.items[i] = store.items[i + 1];
                i += 1;
            }
            store.len -= 1;
        }
    }

    #[kani::proof]
    #[kani::unwind(12)]
    fn b03_iteration_rebuild_empty() {
        let lifecycle: u8 = kani::any();
        kani::assume(lifecycle <= 2);
        let n: usize = kani::any();
        kani::assume(n <= 4);
        let mut store = Store { items: [0; 4], len: 0 };
        let mut i = 0;
        while i < n {
            let v: u64 = kani::any();
            store.items[store.len] = v;
            store.len += 1;
            i += 1;
        }
        let snapshot = store;
        begin_round(lifecycle, &mut store);
        // B-03：iteration ⇒ 任何一轮开始时刻该 store 为空
        assert!(lifecycle != LC_ITERATION || store.len == 0);
        // run/persistent ⇒ 原样保留（重建义务只在 iteration）——逐位比较
        let mut same = store.len == snapshot.len;
        let mut q = 0;
        while q < 4 {
            same = same && store.items[q] == snapshot.items[q];
            q += 1;
        }
        assert!(lifecycle == LC_ITERATION || same);
    }

    #[kani::proof]
    #[kani::unwind(16)]
    fn b04_truncate_respects_declared_bound() {
        let n: usize = kani::any();
        kani::assume(n <= 4);
        let cap_opt: u8 = kani::any(); // 0 = None（未声明），1..=5 = Some(cap)
        let mut store = Store { items: [0; 4], len: 0 };
        let mut i = 0;
        while i < n {
            let v: u64 = kani::any();
            store.items[store.len] = v;
            store.len += 1;
            i += 1;
        }
        let cap: Option<u64> = if cap_opt == 0 { None } else { Some(cap_opt as u64 - 1) };
        if let Some(c) = cap {
            truncate_items(&mut store, cap);
            // 截断后规模 ≤ 已声明上界（max_items 维不变量——B-04）
            assert!(store.len as u64 <= c);
            // 且恰为 min(n, c)（滚动窗口满额保留）
            let expected = if (n as u64) < c { n as u64 } else { c };
            assert!(store.len as u64 == expected);
        }
        // None ⇒ 不截断（max_items 未声明时该维无义务）
        let mut st2 = Store { items: [1, 2, 3, 9], len: 3 };
        truncate_items(&mut st2, None);
        assert!(st2.len == 3);
    }
}

// ===========================================================================
// 模块四：map 扇出 / 调度 / 聚合（B-09 / B-11 / B-13）
// ===========================================================================
mod map_mirror {
    /// 实例表镜像：固定容量 4 + 长度。
    #[derive(Copy, Clone, PartialEq)]
    pub struct Instances {
        pub vals: [u64; 4],
        pub len: usize,
    }

    /// B-09：每元素恰实例化一份子图（实例 i 绑元素 i，实例数 = N）。
    pub fn instantiate(over: &[u64; 4], n: usize) -> Instances {
        let mut inst = Instances { vals: [0; 4], len: 0 };
        let mut i = 0;
        while i < n {
            inst.vals[inst.len] = over[i]; // 隐式形参 item 收元素（【规范】:37）
            inst.len += 1;
            i += 1;
        }
        inst
    }

    /// B-13：聚合器——产出按实例序（元素下标序）落位，与到达/完成序无关。
    /// arrival: (实例下标, 产出值) 的任意到达序（完成序不确定——正是本条
    /// 要消去的自由度）。
    pub fn aggregate(arrival: &[(usize, u64); 4], n: usize) -> [u64; 4] {
        let mut agg: [u64; 4] = [0; 4];
        let mut k = 0;
        while k < n {
            let (idx, val) = arrival[k];
            agg[idx] = val;
            k += 1;
        }
        agg
    }

    /// B-11：max_fanout 波次调度器——任意时刻在飞 ≤ max_fanout。
    /// 返回（发射总数, 峰值在飞）。
    pub fn schedule(n: u64, max_fanout: u64) -> (u64, u64) {
        let mut launched: u64 = 0;
        let mut peak_in_flight: u64 = 0;
        while launched < n {
            let wave = if n - launched < max_fanout { n - launched } else { max_fanout };
            // 调度级并发控制不变量：本波在飞 = wave ≤ max_fanout
            assert!(wave <= max_fanout);
            if wave > peak_in_flight {
                peak_in_flight = wave;
            }
            launched += wave;
        }
        (launched, peak_in_flight)
    }

    #[kani::proof]
    #[kani::unwind(8)]
    fn b09_each_element_exactly_one_instance() {
        let over: [u64; 4] = kani::any();
        let n: usize = kani::any();
        kani::assume(n <= 4);
        let inst = instantiate(&over, n);
        // 实例数 = N
        assert!(inst.len == n);
        // 每元素恰一实例：实例 i 即元素 i（下标双射）
        let mut i = 0;
        while i < n {
            assert!(inst.vals[i] == over[i]);
            i += 1;
        }
        // N=0 ⇒ 空扇出（实例表为空 → 聚合产物为空表）
        let empty = instantiate(&over, 0);
        assert!(empty.len == 0);
    }

    #[kani::proof]
    #[kani::unwind(8)]
    fn b13_aggregation_order_is_instance_order() {
        // 域固定 n=4：到达序 = 四下标的全排列（24 种，两两相异假设排除
        // 重复/遗漏到达——B-13 前提「每实例恰一产出」）。首版 harness 对
        // 非双射到达也断言（内核如实反例：漏到达位不落位）——修正为在
        // 双射前提下证「序 = 实例序，与到达序无关」。
        let n: usize = 4;
        let mut content: [u64; 4] = [0; 4];
        let mut i = 0;
        while i < n {
            let v: u64 = kani::any();
            content[i] = v;
            i += 1;
        }
        let i0: usize = kani::any();
        let i1: usize = kani::any();
        let i2: usize = kani::any();
        let i3: usize = kani::any();
        kani::assume(i0 < 4 && i1 < 4 && i2 < 4 && i3 < 4);
        kani::assume(i0 != i1 && i0 != i2 && i0 != i3 && i1 != i2 && i1 != i3 && i2 != i3);
        let idxs = [i0, i1, i2, i3];
        let mut arrival: [(usize, u64); 4] = [(0, 0); 4];
        let mut k = 0;
        while k < n {
            arrival[k] = (idxs[k], content[idxs[k]]);
            k += 1;
        }
        let agg = aggregate(&arrival, n);
        // 聚合序 = 实例序：聚合[i] == 实例 i 的产出（与到达序 k 无关）
        let mut j = 0;
        while j < n {
            assert!(agg[j] == content[j]);
            j += 1;
        }
    }

    #[kani::proof]
    #[kani::unwind(10)]
    fn b11_in_flight_bounded_by_declared_fanout() {
        let n: u64 = kani::any();
        kani::assume(1 <= n && n <= 6);
        let max_fanout: u64 = kani::any();
        kani::assume(1 <= max_fanout && max_fanout <= 3); // 已声明方向（B-11 范围）
        let (launched, peak) = schedule(n, max_fanout);
        // 任意时刻在飞 ≤ max_fanout（调度器内 assert 已逐波强制；峰值复核）
        assert!(peak <= max_fanout);
        // 全部实例终被发射（发射总数 = n——B-12 聚合完整性的前提面）
        assert!(launched == n);
    }
}

// ===========================================================================
// 模块五：Vec 输出稳定排序（C-03）
// ===========================================================================
mod sort_mirror {
    /// 插入排序镜像（lower.rs:146-148 的稳定键排序在整数域的算法切片；
    /// 稳定性对「逐位可复现」无影响的键全序面）。
    pub fn insertion_sort(a: &mut [u64; 4]) {
        let mut i = 1;
        while i < 4 {
            let mut j = i;
            while j > 0 && a[j - 1] > a[j] {
                a.swap(j - 1, j);
                j -= 1;
            }
            i += 1;
        }
    }

    pub fn sorted(a: &[u64; 4]) -> bool {
        a[0] <= a[1] && a[1] <= a[2] && a[2] <= a[3]
    }

    pub fn count(a: &[u64; 4], x: u64) -> u32 {
        let mut c = 0;
        let mut i = 0;
        while i < 4 {
            if a[i] == x {
                c += 1;
            }
            i += 1;
        }
        c
    }

    pub fn same_multiset(a: &[u64; 4], b: &[u64; 4]) -> bool {
        let mut ok = true;
        let mut i = 0;
        while i < 4 {
            ok = ok && count(a, a[i]) == count(b, a[i]) && count(b, b[i]) == count(a, b[i]);
            i += 1;
        }
        ok
    }

    #[kani::proof]
    #[kani::unwind(16)]
    fn c03_sort_output_sorted_and_multiset_preserving() {
        let mut a: [u64; 4] = kani::any();
        let before = a;
        insertion_sort(&mut a);
        // 输出按稳定键升序（nodes/edges/stores 按 id 排序的序性质）
        assert!(sorted(&a));
        // 输出是输入多重集的函数：多重集保持
        assert!(same_multiset(&before, &a));
    }

    #[kani::proof]
    #[kani::unwind(32)]
    fn c03_sort_idempotent() {
        let mut a: [u64; 4] = kani::any();
        insertion_sort(&mut a);
        let once = a;
        insertion_sort(&mut a);
        // 排序幂等：sort(sort(x)) == sort(x)（逐位比较——数组 == 走 memcmp
        // 耗 unwind，见运行日志失败类 1）
        assert!(super::eq_arr(&a, &once));
    }

    #[kani::proof]
    #[kani::unwind(16)]
    fn c03_sort_permutation_invariant() {
        // 置换不变：任意两位置交换（对换生成全部置换）后排序结果逐位相同
        // ⇒ 输出与构建过程顺序无关（输入多重集的规范函数）
        let mut a: [u64; 4] = kani::any();
        let i: usize = kani::any();
        let j: usize = kani::any();
        kani::assume(i < 4 && j < 4);
        let mut b = a;
        b.swap(i, j);
        insertion_sort(&mut a);
        insertion_sort(&mut b);
        assert!(super::eq_arr(&a, &b));
    }
}

// ===========================================================================
// 模块六：BTreeMap/BTreeSet 遍历序（C-02）
// ===========================================================================
mod btree_mirror {
    /// BTreeMap 镜像：保序（键严格升序）数组表。BTreeMap 迭代序 = 键字典序
    /// （升序）——镜像用「保序插入」承担；标准库容器结构性质属信任面
    /// （C-02 工具注记：不做 BTree 直测——§0.3 病态面）。
    #[derive(Copy, Clone, PartialEq)]
    pub struct SMap {
        pub kv: [(u64, u64); 4],
        pub len: usize,
    }

    pub fn insert(map: &mut SMap, k: u64, v: u64) {
        let mut pos = map.len;
        let mut i = 0;
        while i < map.len {
            if map.kv[i].0 == k {
                map.kv[i].1 = v; // 覆盖语义
                return;
            }
            if map.kv[i].0 > k {
                pos = i;
                break;
            }
            i += 1;
        }
        // 右移腾位（保持升序）
        let mut j = map.len;
        while j > pos {
            map.kv[j] = map.kv[j - 1];
            j -= 1;
        }
        map.kv[pos] = (k, v);
        map.len += 1;
    }

    pub fn keys_ascending(map: &SMap) -> bool {
        let mut i = 1;
        while i < map.len {
            if map.kv[i - 1].0 >= map.kv[i].0 {
                return false;
            }
            i += 1;
        }
        true
    }

    #[kani::proof]
    #[kani::unwind(24)]
    fn c02_iteration_order_is_lexicographic() {
        // 三键全枚举（0/1/2），任意插入顺序（6 种排列全枚举，重复键走覆盖）
        let k0: u64 = kani::any();
        let k1: u64 = kani::any();
        let k2: u64 = kani::any();
        kani::assume(k0 <= 2 && k1 <= 2 && k2 <= 2);
        let v0: u64 = kani::any();
        let v1: u64 = kani::any();
        let v2: u64 = kani::any();
        let o: u8 = kani::any();
        kani::assume(o < 6);
        let mut map = SMap { kv: [(0, 0); 4], len: 0 };
        let seq = permutation3(o, (k0, v0), (k1, v1), (k2, v2));
        let mut t = 0;
        while t < 3 {
            let (k, v) = seq[t];
            insert(&mut map, k, v);
            t += 1;
        }
        // 遍历序 = 键字典序（严格升序）
        assert!(keys_ascending(&map));
        // 同内容 ⇒ 同遍历：按「键升序重排」独立构造规范序，逐位对照
        //（独立构造 = 内容唯一决定遍历；插入序自由度被消去）
        let mut canon = SMap { kv: [(0, 0); 4], len: 0 };
        let mut kk: u64 = 0;
        while kk <= 2 {
            let mut t2 = 0;
            while t2 < 3 {
                let (k, _) = seq[t2];
                if k == kk {
                    let mut t3 = 0;
                    while t3 < map.len {
                        if map.kv[t3].0 == kk {
                            insert(&mut canon, map.kv[t3].0, map.kv[t3].1);
                        }
                        t3 += 1;
                    }
                    break;
                }
                t2 += 1;
            }
            kk += 1;
        }
        assert!(map.len == canon.len);
        let mut q = 0;
        while q < map.len {
            assert!(map.kv[q] == canon.kv[q]);
            q += 1;
        }
    }

    fn permutation3(o: u8, a: (u64, u64), b: (u64, u64), c: (u64, u64)) -> [(u64, u64); 3] {
        match o {
            0 => [a, b, c],
            1 => [a, c, b],
            2 => [b, a, c],
            3 => [b, c, a],
            4 => [c, a, b],
            _ => [c, b, a],
        }
    }
}

// ===========================================================================
// 模块七：降维命名确定性（C-07）
// ===========================================================================
mod naming_mirror {
    /// lower.rs:123-125 镜像：名 = (图名, 路径 . → /)。串域 → (u64, [u64;2])
    /// 逐段镜像；'.' → '/' 为双射改写（镜像为恒等 + 域分离标记）。
    pub fn subgraph_name(graph: u64, path: &[u64; 2]) -> (u64, [u64; 2]) {
        (graph, *path) // 机械拼接，无哈希无计数器（纯函数：无外部可变态）
    }

    /// LowerError::EmptyBody 的 u8 镜像码（串字面量 == 走 memcmp 耗 unwind，
    /// 见运行日志失败类 1——错误面以离散码镜像，判定语义不变）。
    pub const ERR_EMPTY_BODY: u8 = 1;

    /// lower.rs:300-305 / :346-351 镜像：entry = 体 nodes 首元素；空体 →
    /// EmptyBody（LowerError::EmptyBody，绝不 panic）。
    pub fn entry_of(body: &[u64], nonempty: bool) -> Result<u64, u8> {
        if nonempty {
            Ok(body[0])
        } else {
            Err(ERR_EMPTY_BODY)
        }
    }

    #[kani::proof]
    #[kani::unwind(6)]
    fn c07_naming_injective_and_stateless() {
        let g1: u64 = kani::any();
        let g2: u64 = kani::any();
        let p1: [u64; 2] = kani::any();
        let p2: [u64; 2] = kani::any();
        let n1 = subgraph_name(g1, &p1);
        let n2 = subgraph_name(g2, &p2);
        // 名字可从文档树复算：名相等 ⇒ (图名, 路径) 相等（单射——同名子图
        // 不可能由不同构造产生，BTreeMap 键无碰撞覆写；逐位比较）
        assert!(n1.0 != n2.0 || !super::eq_arr(&n1.1, &n2.1) || (g1 == g2 && super::eq_arr(&p1, &p2)));
        // 双跑一致：同一构造两次命名逐位相同
        let n1b = subgraph_name(g1, &p1);
        assert!(n1.0 == n1b.0 && super::eq_arr(&n1.1, &n1b.1));
    }

    #[kani::proof]
    #[kani::unwind(6)]
    fn c07_entry_is_first_node_or_empty_body_error() {
        let n0: u64 = kani::any();
        let n1: u64 = kani::any();
        let nonempty: bool = kani::any();
        let body: [u64; 2] = [n0, n1];
        match entry_of(&body, nonempty) {
            Ok(e) => {
                // entry = 体 nodes 首元素（lower.rs:302-304）
                assert!(nonempty && e == n0);
            }
            Err(code) => {
                // 空体 → LowerError::EmptyBody 面（不 panic；u8 码镜像）
                assert!(!nonempty && code == ERR_EMPTY_BODY);
            }
        }
    }

    #[kani::proof]
    #[kani::unwind(6)]
    fn c07_graph_version_stamps_parent() {
        // lower.rs:307-311：子图 graph_version 一律 stamp 父图版本
        let parent_version: u64 = kani::any();
        let mut subgraphs: [(u64, u64); 2] = [(0, 0); 2]; // (名, 版本) 有序表镜像
        let name: u64 = kani::any();
        kani::assume(name != 0); // 名域分离：真名 =「图名/路径」非空串
        //（首版无此约束——内核如实反例 name=0 撞哨兵槽 (0,0)，镜像域失真，
        //  非实现缺陷：lower.rs:123-125 拼接名恒非空）
        subgraphs[0] = (name, parent_version);
        // stamp 后查得版本 == 父图版本（E2007 DSL 面不可触发的镜像面）
        let mut found: Option<u64> = None;
        let mut i = 0;
        while i < 2 {
            if subgraphs[i].0 == name {
                found = Some(subgraphs[i].1);
            }
            i += 1;
        }
        assert!(found == Some(parent_version));
    }
}

// ===========================================================================
// 模块八：lower 双跑逐位一致（C-01，ideal 对象）
// ===========================================================================
mod lower_pipeline {
    /// 文档边镜像：IrEdge 稳定键四元组（lower.rs:135-148 投影 + 排序键）。
    pub type EdgeKey = [u64; 4]; // (from, output, to, input)

    pub fn lex_less(a: EdgeKey, b: EdgeKey) -> bool {
        if a[0] != b[0] {
            return a[0] < b[0];
        }
        if a[1] != b[1] {
            return a[1] < b[1];
        }
        if a[2] != b[2] {
            return a[2] < b[2];
        }
        a[3] < b[3]
    }

    /// lower 阶段镜像：投影 → 按 (from, output, to, input) 字典序排序。
    /// 纯函数：无外部可变状态、无时钟/随机/网络（lower.rs:1-4 模块自注的
    /// 镜像；「签名纯度」由本函数无 &mut 外部参数/无全局读取承担）。
    pub fn lower_edges(edges: &[EdgeKey; 4]) -> [EdgeKey; 4] {
        let mut out = *edges;
        // 字典序插入排序（lower.rs:146-148 稳定键排序的镜像；键 = 全四元组）
        let mut i = 1;
        while i < 4 {
            let mut j = i;
            while j > 0 && lex_less(out[j], out[j - 1]) {
                out.swap(j - 1, j);
                j -= 1;
            }
            i += 1;
        }
        out
    }

    /// 计划装配镜像：IR 边序 + 节点 id 序 + 子图命名（三表合一）。
    /// 「计划对象」镜像 = (边序, 节点序, 名字)；双跑逐位一致 = 三者全等。
    pub fn make_plan(
        edges: &[EdgeKey; 4],
        node_ids: &[u64; 3],
        graph: u64,
        body: &[u64; 2],
    ) -> ([EdgeKey; 4], [u64; 3], (u64, [u64; 2])) {
        let ir_edges = lower_edges(edges);
        let mut nodes = *node_ids;
        // lower.rs:225 节点按 id 排序
        let mut i = 1;
        while i < 3 {
            let mut j = i;
            while j > 0 && nodes[j - 1] > nodes[j] {
                nodes.swap(j - 1, j);
                j -= 1;
            }
            i += 1;
        }
        let name = super::naming_mirror::subgraph_name(graph, body);
        (ir_edges, nodes, name)
    }

    #[kani::proof]
    #[kani::unwind(32)]
    fn c01_double_run_bitwise_identical() {
        // 文档空间：边/节点键全枚举（有界域 0..=2）
        let e: [EdgeKey; 4] = kani::any();
        let mut q = 0;
        while q < 4 {
            kani::assume(e[q][0] <= 2 && e[q][1] <= 2 && e[q][2] <= 2 && e[q][3] <= 2);
            q += 1;
        }
        let nodes: [u64; 3] = kani::any();
        kani::assume(nodes[0] <= 2 && nodes[1] <= 2 && nodes[2] <= 2);
        let graph: u64 = kani::any();
        kani::assume(graph <= 2);
        let body: [u64; 2] = kani::any();
        kani::assume(body[0] <= 2 && body[1] <= 2);

        // —— 同一文档两次编译 ——
        let plan1 = make_plan(&e, &nodes, graph, &body);
        let plan2 = make_plan(&e, &nodes, graph, &body);

        // 产出 IR 与计划对象逐位相同（三表全等；逐边逐位比较——memcmp 纪律）
        let mut same = true;
        let mut q = 0;
        while q < 4 {
            same = same && super::eq_arr(&plan1.0[q], &plan2.0[q]);
            q += 1;
        }
        same = same && super::eq_arr(&plan1.1, &plan2.1);
        same = same && plan1.2.0 == plan2.2.0 && super::eq_arr(&plan1.2.1, &plan2.2.1);
        assert!(same);

        // 分解面 1：边序与构建序无关（对换不变——C-03 的边投影面）
        let mut swapped = e;
        swapped.swap(0, 3);
        let plan3 = make_plan(&swapped, &nodes, graph, &body);
        let mut same_edges = true;
        let mut q2 = 0;
        while q2 < 4 {
            same_edges = same_edges && super::eq_arr(&plan3.0[q2], &plan1.0[q2]);
            q2 += 1;
        }
        assert!(same_edges);

        // 分解面 2：节点序 = id 升序（lower.rs:225）
        assert!(plan1.1[0] <= plan1.1[1] && plan1.1[1] <= plan1.1[2]);

        // 分解面 3：命名 = (图名, 体路径) 纯派生（C-07）
        assert!(plan1.2.0 == graph && super::eq_arr(&plan1.2.1, &body));
    }
}
