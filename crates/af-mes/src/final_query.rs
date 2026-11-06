//! 读路径：`SELECT … FROM 表 FINAL`。
//!
//! poc-mes 的坑：`OPTIMIZE TABLE … FINAL` 只在分区内合并。月分区上的
//! ReplacingMergeTree 残留（同一排序键落在不同月）靠 OPTIMIZE 去不掉。
//! 全局去重是读时的 `FINAL`。事件表本身是 MergeTree，没有版本列；
//! 包装器仍强制 `FINAL`，这样读 ReplacingMergeTree 时不会退回 OPTIMIZE。
//!
//! `rand64()` 不是纯函数，片段里出现就拒绝。

use crate::error::MesError;

/// 生成 `SELECT {columns} FROM {table} FINAL [WHERE …]`。
///
/// `table` 是裸标识符或 `db.table`。`columns` 与 `where_clause` 不得含
/// 语句分隔、注释、`rand64` 或 `OPTIMIZE`。
pub fn select_final(
    table: &str,
    columns: &str,
    where_clause: Option<&str>,
) -> Result<String, MesError> {
    validate_ident(table)?;
    let columns = columns.trim();
    if columns.is_empty() {
        return Err(MesError::BadQuery("列不能为空".into()));
    }
    validate_fragment(columns, "列")?;

    let mut sql = format!("SELECT {columns} FROM {table} FINAL");
    if let Some(clause) = where_clause {
        let clause = clause.trim();
        if !clause.is_empty() {
            validate_fragment(clause, "WHERE")?;
            sql.push_str(" WHERE ");
            sql.push_str(clause);
        }
    }
    guard_read_sql(&sql)?;
    Ok(sql)
}

/// 拒绝把 `OPTIMIZE … FINAL` 或 `rand64` 当作读语句。
pub fn guard_read_sql(sql: &str) -> Result<(), MesError> {
    let folded = sql.to_ascii_lowercase();
    if folded.contains("rand64") {
        return Err(MesError::BadQuery(
            "rand64() 不是纯函数，读路径不生成它".into(),
        ));
    }
    if folded.contains("optimize") {
        return Err(MesError::BadQuery(
            "OPTIMIZE FINAL 不跨分区合并；读 ReplacingMergeTree 残留用 select_final".into(),
        ));
    }
    Ok(())
}

fn validate_ident(table: &str) -> Result<(), MesError> {
    let mut parts = table.split('.');
    let Some(first) = parts.next() else {
        return Err(MesError::BadQuery("表名不能为空".into()));
    };
    if !is_ident(first) {
        return Err(MesError::BadQuery(format!("非法表名: {table}")));
    }
    if let Some(second) = parts.next() {
        if !is_ident(second) || parts.next().is_some() {
            return Err(MesError::BadQuery(format!("非法表名: {table}")));
        }
    }
    Ok(())
}

fn is_ident(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn validate_fragment(fragment: &str, what: &str) -> Result<(), MesError> {
    let folded = fragment.to_ascii_lowercase();
    if fragment.contains(';')
        || fragment.contains("--")
        || fragment.contains("/*")
        || fragment.contains("*/")
        || fragment.contains('?')
    {
        return Err(MesError::BadQuery(format!("{what} 含有不允许的片段")));
    }
    if folded.contains("rand64") {
        return Err(MesError::BadQuery(
            "rand64() 不是纯函数，读路径不生成它".into(),
        ));
    }
    if folded.contains("optimize") {
        return Err(MesError::BadQuery(
            "OPTIMIZE FINAL 不跨分区合并；读 ReplacingMergeTree 残留用 select_final".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_table_with_final_and_optional_where() {
        let sql = select_final("mes_event", "work_order_id, occurred_at", None).unwrap();
        assert_eq!(
            sql,
            "SELECT work_order_id, occurred_at FROM mes_event FINAL"
        );

        let sql = select_final("af_mes.mes_event", "*", Some("work_order_id = 'wo'")).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM af_mes.mes_event FINAL WHERE work_order_id = 'wo'"
        );
        assert_eq!(sql.matches(" FINAL").count(), 1);
        assert!(!sql.to_ascii_lowercase().contains("optimize"));
    }

    #[test]
    fn rejects_optimize_final_and_rand64() {
        let err = guard_read_sql("OPTIMIZE TABLE mes_event FINAL").unwrap_err();
        assert!(err.to_string().contains("不跨分区"));

        let err = select_final("mes_event", "rand64()", None).unwrap_err();
        assert!(err.to_string().contains("rand64"));

        let err = select_final("mes_event", "event_id", Some("quantity > rand64()")).unwrap_err();
        assert!(err.to_string().contains("rand64"));

        assert!(select_final("mes_event;drop", "*", None).is_err());
        assert!(select_final("mes_event", "event_id; --", None).is_err());
        assert!(select_final("mes event", "*", None).is_err());
    }
}
