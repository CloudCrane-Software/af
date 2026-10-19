//! 跨 crate 词表：af-credential 签发的美分限额原样进入 af-meter 账本。
//!
//! `af_credential::Cents` 与 `af_meter::Cents` 都是 `i64`。
//! 下面两个函数的签名过不了编译，就说明两边的金额类型分叉了。

use af_credential::{attenuate, authorize, inspect, issue_root, AccessQuery, ChildSpec, RootSpec};
use af_meter::{BudgetError, BudgetLedger};

/// 凭证上的 `budget_cents` 直接成为账本总额。
fn ledger_from_issued(budget_cents: af_credential::Cents) -> BudgetLedger {
    BudgetLedger::new(budget_cents)
}

/// 凭证上的 `spend_cents` 与 `spend_cap_cents` 直接交给 `try_spend`。
fn spend_issued(
    ledger: &mut BudgetLedger,
    spend_cents: af_credential::Cents,
    spend_cap_cents: af_credential::Cents,
) -> Result<(), BudgetError> {
    ledger.try_spend(spend_cents, spend_cap_cents)
}

#[test]
fn credential_limit_consumed_without_unit_conversion() {
    let budget_cents: af_credential::Cents = 1_000;
    let spend_cap_cents: af_credential::Cents = 500;
    let spend_ok: af_credential::Cents = 300;
    let spend_over_cap: af_credential::Cents = 800;

    // 计数是另一套词：200 次调用的类型是 u64，不是美分。
    let calls: af_meter::Count = 200u64;
    assert_eq!(calls, 200);

    let root = issue_root(&RootSpec {
        tools: vec!["test".into()],
        namespace: "line-a".into(),
        budget_cents,
    })
    .unwrap();
    let child = attenuate(
        &root.token,
        &ChildSpec {
            tools: vec!["test".into()],
            spend_cap_cents,
        },
    )
    .unwrap();

    let caps = inspect(&child).unwrap();
    assert_eq!(caps.budget_cents, Some(budget_cents));
    assert!(
        caps.block_sources[0].contains("budget_cents(1000)"),
        "authority={}",
        caps.block_sources[0]
    );
    assert_eq!(af_credential::F_BUDGET_CENTS, "budget_cents");
    assert_eq!(af_credential::F_SPEND_CENTS, "spend_cents");

    let mut ledger = ledger_from_issued(caps.budget_cents.unwrap());
    assert_eq!(ledger.budget_cents, budget_cents);
    assert_eq!(ledger.remaining_cents(), budget_cents);
    assert_eq!(ledger.spent_cents, 0);

    let q = AccessQuery {
        role: "reviewer".into(),
        tool: "test".into(),
        namespace: "line-a".into(),
        spend_cents: spend_ok,
    };

    // 300 <= cap 500，且 300 <= 根 budget 1000：凭证放行，账本记 300。
    assert!(authorize(&child, &q.tool, &q.namespace, q.spend_cents).unwrap());
    spend_issued(&mut ledger, q.spend_cents, spend_cap_cents).unwrap();
    assert_eq!(ledger.spent_cents, spend_ok);
    assert_eq!(ledger.remaining_cents(), 700);

    // 800 > cap 500：两边都拒绝，账本不动。
    assert!(!authorize(&child, "test", "line-a", spend_over_cap).unwrap());
    let err = spend_issued(&mut ledger, spend_over_cap, spend_cap_cents).unwrap_err();
    assert_eq!(
        err,
        BudgetError::OverCap {
            spend_cents: spend_over_cap,
            spend_cap_cents,
        }
    );
    assert_eq!(ledger.remaining_cents(), 700);

    // 再两笔 300：凭证每笔都放行，账本累计到剩 100。
    for expected_remaining in [400_i64, 100] {
        assert!(authorize(&child, "test", "line-a", spend_ok).unwrap());
        spend_issued(&mut ledger, spend_ok, spend_cap_cents).unwrap();
        assert_eq!(ledger.remaining_cents(), expected_remaining);
    }

    // 累计超预算：凭证仍放行（它只做 per-call），账本拒绝且不扣减。
    assert!(authorize(&child, "test", "line-a", spend_ok).unwrap());
    let err = spend_issued(&mut ledger, spend_ok, spend_cap_cents).unwrap_err();
    assert_eq!(
        err,
        BudgetError::OverBudget {
            spend_cents: spend_ok,
            remaining_cents: 100,
        }
    );
    assert_eq!(ledger.spent_cents, 900);
    assert_eq!(ledger.remaining_cents(), 100);

    // 剩余 100 本身就是美分余额，原样再花一笔。100 <= cap 500，两边都放行。
    let rest: af_credential::Cents = ledger.remaining_cents();
    assert_eq!(rest, 100);
    assert!(authorize(&child, "test", "line-a", rest).unwrap());
    spend_issued(&mut ledger, rest, spend_cap_cents).unwrap();
    assert_eq!(ledger.remaining_cents(), 0);
    assert_eq!(ledger.spent_cents, budget_cents);
}
