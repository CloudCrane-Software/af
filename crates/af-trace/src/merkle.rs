//! RFC 6962 Merkle 树。
//!
//! 叶子是 `SHA-256(0x00 || data)`。内部节点是 `SHA-256(0x01 || left || right)`。
//! `n > 1` 时按严格小于 `n` 的最大 2 的幂拆开，奇数层的末节点提升，不复制。

use crate::sha256::sha256;

pub(crate) fn leaf_hash(data: &[u8]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(1 + data.len());
    buf.push(0x00);
    buf.extend_from_slice(data);
    sha256(&buf)
}

pub(crate) fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut buf = [0u8; 65];
    buf[0] = 0x01;
    buf[1..33].copy_from_slice(left);
    buf[33..65].copy_from_slice(right);
    sha256(&buf)
}

/// 空输入返回 `None`。非空时 `entries` 是叶子的原始字节，函数自己加 `0x00` 前缀。
pub(crate) fn merkle_root(entries: &[&[u8]]) -> Option<[u8; 32]> {
    if entries.is_empty() {
        return None;
    }
    let leaves: Vec<[u8; 32]> = entries.iter().map(|entry| leaf_hash(entry)).collect();
    Some(root_from_leaf_hashes(&leaves))
}

pub(crate) fn root_from_leaf_hashes(leaves: &[[u8; 32]]) -> [u8; 32] {
    fn mth(leaves: &[[u8; 32]]) -> [u8; 32] {
        match leaves.len() {
            0 => sha256(b""),
            1 => leaves[0],
            n => {
                let k = largest_pow2_less_than(n);
                node_hash(&mth(&leaves[..k]), &mth(&leaves[k..]))
            }
        }
    }
    mth(leaves)
}

/// 纳入证明，从叶子往根。空树或下标越界返回 `None`。单叶子的证明是空列表。
/// 生产路径只验证明；组证明的是本 crate 的 rekor stub。
#[cfg(test)]
pub(crate) fn inclusion_proof(leaves: &[[u8; 32]], index: usize) -> Option<Vec<[u8; 32]>> {
    if index >= leaves.len() || leaves.is_empty() {
        return None;
    }
    fn path(leaves: &[[u8; 32]], index: usize) -> Vec<[u8; 32]> {
        let n = leaves.len();
        if n == 1 {
            return Vec::new();
        }
        let k = largest_pow2_less_than(n);
        if index < k {
            let mut proof = path(&leaves[..k], index);
            proof.push(root_from_leaf_hashes(&leaves[k..]));
            proof
        } else {
            let mut proof = path(&leaves[k..], index - k);
            proof.push(root_from_leaf_hashes(&leaves[..k]));
            proof
        }
    }
    Some(path(leaves, index))
}

/// 用纳入证明把叶子摘要折回树根。失败返回 `None`。
pub(crate) fn eval_inclusion(
    leaf: &[u8; 32],
    index: u64,
    tree_size: u64,
    proof: &[[u8; 32]],
) -> Option<[u8; 32]> {
    if tree_size == 0 || index >= tree_size {
        return None;
    }
    fn sub(leaf: &[u8; 32], index: u64, size: u64, proof: &[[u8; 32]]) -> Option<[u8; 32]> {
        if size == 1 {
            return if proof.is_empty() && index == 0 {
                Some(*leaf)
            } else {
                None
            };
        }
        let (sibling, rest) = proof.split_last()?;
        let k = largest_pow2_less_than_u64(size)?;
        if index < k {
            let left = sub(leaf, index, k, rest)?;
            Some(node_hash(&left, sibling))
        } else {
            let right = sub(leaf, index - k, size - k, rest)?;
            Some(node_hash(sibling, &right))
        }
    }
    sub(leaf, index, tree_size, proof)
}

fn largest_pow2_less_than(n: usize) -> usize {
    debug_assert!(n >= 2);
    1usize << (n - 1).ilog2()
}

fn largest_pow2_less_than_u64(n: u64) -> Option<u64> {
    if n < 2 {
        return None;
    }
    (n - 1).checked_ilog2().map(|bits| 1u64 << bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_leaf_root_is_prefixed_hash_and_proof_is_empty() {
        let data = b"event-0";
        let root = merkle_root(&[data]).unwrap();
        assert_eq!(root, leaf_hash(data));
        let leaves = [leaf_hash(data)];
        assert!(inclusion_proof(&leaves, 0).unwrap().is_empty());
        assert_eq!(
            eval_inclusion(&leaves[0], 0, 1, &[]).unwrap(),
            root_from_leaf_hashes(&leaves)
        );
    }

    #[test]
    fn two_leaves_are_one_internal_node() {
        let a = leaf_hash(b"a");
        let b = leaf_hash(b"b");
        let leaves = [a, b];
        assert_eq!(root_from_leaf_hashes(&leaves), node_hash(&a, &b));
        let proof0 = inclusion_proof(&leaves, 0).unwrap();
        assert_eq!(proof0, vec![b]);
        assert_eq!(
            eval_inclusion(&a, 0, 2, &proof0).unwrap(),
            node_hash(&a, &b)
        );
        let proof1 = inclusion_proof(&leaves, 1).unwrap();
        assert_eq!(proof1, vec![a]);
        assert_eq!(
            eval_inclusion(&b, 1, 2, &proof1).unwrap(),
            node_hash(&a, &b)
        );
    }

    #[test]
    fn inclusion_matches_root_for_uneven_trees() {
        for n in 1..=9 {
            let leaves: Vec<[u8; 32]> = (0..n)
                .map(|i| leaf_hash(format!("leaf-{i}").as_bytes()))
                .collect();
            let root = root_from_leaf_hashes(&leaves);
            let size = u64::try_from(n).unwrap();
            for index in 0..n {
                let proof = inclusion_proof(&leaves, index).unwrap();
                let got =
                    eval_inclusion(&leaves[index], u64::try_from(index).unwrap(), size, &proof)
                        .unwrap();
                assert_eq!(got, root, "n={n} index={index}");
            }
            assert!(inclusion_proof(&leaves, n).is_none());
            assert!(eval_inclusion(&leaves[0], size, size, &[]).is_none());
        }
    }

    #[test]
    fn flipped_sibling_does_not_rebuild_the_root() {
        let leaves: Vec<[u8; 32]> = (0..5).map(|i| leaf_hash(&[i])).collect();
        let root = root_from_leaf_hashes(&leaves);
        let mut proof = inclusion_proof(&leaves, 3).unwrap();
        proof[0][0] ^= 0xff;
        let got = eval_inclusion(&leaves[3], 3, 5, &proof).unwrap();
        assert_ne!(got, root);
    }
}
