//! 冻结快照的规范字节与污染交集。
//!
//! 哈希覆盖谱系、版本和按 `case_id` 排序后的用例。同一组内容顺序不同，摘要相同；
//! 版本或任一 payload 变了，摘要就变。基准集是 payload 的精确成员，不是子串。

use std::collections::BTreeSet;

use crate::error::{EvalError, Result};
use crate::hash::{sha256_hex, SHA256_HEX_LEN};

/// 一道基准用例。`payload` 是污染交集的成员。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchCase {
    pub case_id: String,
    pub payload: String,
}

/// 评测运行允许读取的全部基准内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenSnapshot {
    pub lineage_id: String,
    pub version: u64,
    pub cases: Vec<BenchCase>,
    pub hash: String,
}

/// 规范字节的摘要。`cases` 会按 `case_id` 排序后再编码。
pub fn snapshot_hash(lineage_id: &str, version: u64, cases: &[BenchCase]) -> Result<String> {
    Ok(sha256_hex(&canonical_bytes(lineage_id, version, cases)?))
}

/// 排序后的快照。摘要与 [`snapshot_hash`] 用同一份字节。
pub fn frozen_snapshot(
    lineage_id: &str,
    version: u64,
    cases: &[BenchCase],
) -> Result<FrozenSnapshot> {
    let cases = ordered_cases(cases)?;
    if lineage_id.is_empty() {
        return Err(EvalError::invalid("lineage_id"));
    }
    let hash = sha256_hex(&canonical_from_ordered(lineage_id, version, &cases));
    debug_assert_eq!(hash.len(), SHA256_HEX_LEN);
    Ok(FrozenSnapshot {
        lineage_id: lineage_id.to_owned(),
        version,
        cases,
        hash,
    })
}

/// 运行输入与基准集的精确交集。命中项去重后按字典序返回。空向量表示未污染。
pub fn contamination_overlap(cases: &[BenchCase], input: &[String]) -> Vec<String> {
    let members: BTreeSet<&str> = cases.iter().map(|case| case.payload.as_str()).collect();
    let mut hit = BTreeSet::new();
    for fragment in input {
        if members.contains(fragment.as_str()) {
            hit.insert(fragment.clone());
        }
    }
    hit.into_iter().collect()
}

/// 长度前缀列表。成员可以包含换行。空列表不是合法的污染记录。
pub(crate) fn encode_list(items: &[String]) -> String {
    let mut out = String::new();
    out.push_str(&items.len().to_string());
    out.push('\n');
    for item in items {
        out.push_str(&item.len().to_string());
        out.push('\n');
        out.push_str(item);
        out.push('\n');
    }
    out
}

pub(crate) fn decode_list(text: &str) -> Result<Vec<String>> {
    let mut rest = text;
    let (count_text, next) = rest.split_once('\n').ok_or_else(|| EvalError::Corrupt {
        detail: "列表缺少长度".into(),
    })?;
    rest = next;
    let count: usize = count_text.parse().map_err(|_| EvalError::Corrupt {
        detail: "列表长度无法解析".into(),
    })?;
    if count == 0 || count > 100_000 {
        return Err(EvalError::Corrupt {
            detail: "列表长度超出账本允许的范围".into(),
        });
    }
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        let (len_text, next) = rest.split_once('\n').ok_or_else(|| EvalError::Corrupt {
            detail: "列表项缺少长度".into(),
        })?;
        rest = next;
        let len: usize = len_text.parse().map_err(|_| EvalError::Corrupt {
            detail: "列表项长度无法解析".into(),
        })?;
        let bytes = rest.as_bytes();
        if bytes.len() < len + 1 || bytes[len] != b'\n' {
            return Err(EvalError::Corrupt {
                detail: "列表项被截断".into(),
            });
        }
        let item = std::str::from_utf8(&bytes[..len]).map_err(|_| EvalError::Corrupt {
            detail: "列表项不是 UTF-8".into(),
        })?;
        items.push(item.to_owned());
        rest = &rest[len + 1..];
    }
    if !rest.is_empty() {
        return Err(EvalError::Corrupt {
            detail: "列表尾部有多余字节".into(),
        });
    }
    Ok(items)
}

fn canonical_bytes(lineage_id: &str, version: u64, cases: &[BenchCase]) -> Result<Vec<u8>> {
    let cases = ordered_cases(cases)?;
    if lineage_id.is_empty() {
        return Err(EvalError::invalid("lineage_id"));
    }
    Ok(canonical_from_ordered(lineage_id, version, &cases))
}

fn ordered_cases(cases: &[BenchCase]) -> Result<Vec<BenchCase>> {
    let mut ordered = cases.to_vec();
    ordered.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    for case in &ordered {
        if case.case_id.is_empty() {
            return Err(EvalError::invalid("case_id"));
        }
        if case.payload.is_empty() {
            return Err(EvalError::invalid("payload"));
        }
    }
    for pair in ordered.windows(2) {
        if pair[0].case_id == pair[1].case_id {
            return Err(EvalError::invalid("case_id"));
        }
    }
    Ok(ordered)
}

fn canonical_from_ordered(lineage_id: &str, version: u64, cases: &[BenchCase]) -> Vec<u8> {
    let mut out = Vec::new();
    push_lp(&mut out, b"af-eval.bench.1");
    push_lp(&mut out, lineage_id.as_bytes());
    push_lp(&mut out, version.to_string().as_bytes());
    push_lp(&mut out, cases.len().to_string().as_bytes());
    for case in cases {
        push_lp(&mut out, case.case_id.as_bytes());
        push_lp(&mut out, case.payload.as_bytes());
    }
    out
}

fn push_lp(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(bytes.len().to_string().as_bytes());
    out.push(b'\n');
    out.extend_from_slice(bytes);
    out.push(b'\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(id: &str, payload: &str) -> BenchCase {
        BenchCase {
            case_id: id.to_owned(),
            payload: payload.to_owned(),
        }
    }

    #[test]
    fn hash_ignores_insertion_order_and_tracks_version_and_payload() {
        let left = snapshot_hash("math", 1, &[case("b", "beta"), case("a", "alpha")]).unwrap();
        let right = snapshot_hash("math", 1, &[case("a", "alpha"), case("b", "beta")]).unwrap();
        assert_eq!(left, right);
        assert_eq!(left.len(), SHA256_HEX_LEN);

        let other_version =
            snapshot_hash("math", 2, &[case("a", "alpha"), case("b", "beta")]).unwrap();
        let other_payload =
            snapshot_hash("math", 1, &[case("a", "alpha"), case("b", "beta2")]).unwrap();
        let other_lineage =
            snapshot_hash("geom", 1, &[case("a", "alpha"), case("b", "beta")]).unwrap();
        assert_ne!(left, other_version);
        assert_ne!(left, other_payload);
        assert_ne!(left, other_lineage);
    }

    #[test]
    fn overlap_is_exact_membership_not_substring() {
        let cases = vec![case("c1", "gold-1"), case("c2", "gold-2")];
        assert_eq!(
            contamination_overlap(&cases, &["notes".into(), "gold-2".into(), "gold-2".into()]),
            vec!["gold-2".to_owned()]
        );
        assert!(contamination_overlap(&cases, &["gold".into(), "gold-1-extra".into()]).is_empty());
        assert_eq!(
            contamination_overlap(&cases, &["gold-2".into(), "gold-1".into()]),
            vec!["gold-1".to_owned(), "gold-2".to_owned()]
        );
    }

    #[test]
    fn list_roundtrip_keeps_newlines_and_unicode() {
        let items = vec!["plain".to_owned(), "行\n内".to_owned(), "β".to_owned()];
        let encoded = encode_list(&items);
        assert_eq!(decode_list(&encoded).unwrap(), items);
        assert!(decode_list("").is_err());
        assert!(decode_list("0\n").is_err());
    }
}
