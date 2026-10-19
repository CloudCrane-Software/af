//! critical 批次到私有 rekor-tiles 的提交与验锚。
//!
//! 提交 `POST {endpoint}/api/v2/log/entries`，正文是 rekor v2 的
//! `hashedRekordRequestV002`。`digest` 是批次 Merkle 根的原始 32 字节（标准 base64）。
//! 日志叶子是 `SHA-256(0x00 || merkle_root)`，纳入证明按 RFC 6962。
//!
//! 回执里的签名是检查点信封的 HMAC-SHA256，密钥由调用方注入。
//! 官方 rekor-tiles 的检查点是 Ed25519 note；本 crate 不引入新的签名 crate，
//! 真实日志的连通不在 `cargo test` 里。
//!
//! 传输失败和非 2xx 都是 [`AnchorError::Unreachable`]，调用方持久化后重试。

use crate::event::TraceEvent;
use crate::http_util;
use crate::merkle::{eval_inclusion, leaf_hash, merkle_root};
#[cfg(test)]
use crate::merkle::{inclusion_proof, root_from_leaf_hashes};
use crate::sha256::{b64_decode, b64_encode, ct_eq, hex_decode, hex_encode, hmac_sha256};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const DEFAULT_ORIGIN: &str = "rekor-local";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

/// 一次成功锚定的回执。落在 critical 日志的侧车里，一行一条。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorReceipt {
    /// 批次内事件的 `seq`，顺序与封口时一致。
    pub batch_seqs: Vec<u64>,
    /// 批次 Merkle 根，小写十六进制。提交给 rekor 的 digest 就是这 32 字节。
    pub merkle_root_hex: String,
    /// rekor 日志序号。
    pub log_index: u64,
    /// 生成证明时的树大小。
    pub tree_size: u64,
    /// rekor 树根，小写十六进制。
    pub rekor_root_hex: String,
    /// RFC 6962 纳入证明，叶子到根，小写十六进制。
    pub proof_hashes_hex: Vec<String>,
    /// C2SP 检查点信封，含签名行。
    pub checkpoint: String,
    /// 检查点签名，标准 base64。验锚会把它和信封里的签名对齐。
    pub signature_b64: String,
    /// 提交时注入钟的毫秒。不在检查点签名覆盖范围内。
    pub integrated_time_ms: u64,
}

/// 验锚或提交失败。传输失败用 [`AnchorError::Unreachable`]，不表示 critical 落盘失败。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AnchorError {
    #[error("锚定批次为空")]
    EmptyBatch,
    #[error("回执与批次不一致: {0}")]
    BatchMismatch(String),
    #[error("批次 Merkle 根与回执不一致")]
    RootMismatch,
    #[error("纳入证明与检查点根不一致")]
    ProofMismatch,
    #[error("检查点签名无效")]
    BadSignature,
    #[error("检查点与回执的树状态不一致: {0}")]
    CheckpointMismatch(String),
    #[error("回执无法解析: {0}")]
    Malformed(String),
    #[error("rekor 端点无效: {0}")]
    Endpoint(String),
    #[error("rekor 不可达: {0}")]
    Unreachable(String),
}

/// 按回执复核一批 critical 事件。
///
/// 先重算批次 Merkle 根，再按回执里的纳入路径把 `SHA-256(0x00 || 根)` 折到 rekor 树根，
/// 最后用注入的 HMAC 密钥核对检查点签名。
pub fn verify_anchor(
    batch: &[TraceEvent],
    receipt: &AnchorReceipt,
    hmac_key: &[u8],
    origin: &str,
) -> Result<(), AnchorError> {
    if batch.is_empty() {
        return Err(AnchorError::EmptyBatch);
    }
    let seqs: Vec<u64> = batch.iter().map(|event| event.seq).collect();
    if seqs != receipt.batch_seqs {
        return Err(AnchorError::BatchMismatch(
            "batch_seqs 与事件顺序不一致".into(),
        ));
    }
    let root = batch_root(batch)?;
    if hex_encode(&root) != receipt.merkle_root_hex {
        return Err(AnchorError::RootMismatch);
    }
    let rekor_root = decode_hash(&receipt.rekor_root_hex, "rekor_root_hex")?;
    let proof = decode_proof(&receipt.proof_hashes_hex)?;
    let leaf = leaf_hash(&root);
    let folded = eval_inclusion(&leaf, receipt.log_index, receipt.tree_size, &proof)
        .ok_or(AnchorError::ProofMismatch)?;
    if folded != rekor_root {
        return Err(AnchorError::ProofMismatch);
    }
    verify_checkpoint(
        &receipt.checkpoint,
        origin,
        hmac_key,
        receipt.tree_size,
        &rekor_root,
        &receipt.signature_b64,
    )
}

pub struct RekorClient {
    url: String,
    origin: String,
    hmac_key: Vec<u8>,
    timeout: Duration,
}

impl RekorClient {
    /// `endpoint` 是 `http://host:port` 或已经带 `/api/v2/log/entries` 的 URL。
    pub fn new(endpoint: &str, origin: &str, hmac_key: &[u8]) -> Result<Self, AnchorError> {
        let url = entries_url(endpoint)?;
        let origin = if origin.trim().is_empty() {
            DEFAULT_ORIGIN.to_string()
        } else {
            origin.trim().to_string()
        };
        Ok(Self {
            url,
            origin,
            hmac_key: hmac_key.to_vec(),
            timeout: DEFAULT_TIMEOUT,
        })
    }

    /// 提交一批已经按落盘顺序排好的 critical 事件。成功返回可验的回执。
    pub fn submit_batch(
        &self,
        batch: &[TraceEvent],
        integrated_time_ms: u64,
    ) -> Result<AnchorReceipt, AnchorError> {
        if batch.is_empty() {
            return Err(AnchorError::EmptyBatch);
        }
        let root = batch_root(batch)?;
        let proof = self.submit_root(&root)?;
        let receipt = AnchorReceipt {
            batch_seqs: batch.iter().map(|event| event.seq).collect(),
            merkle_root_hex: hex_encode(&root),
            log_index: proof.log_index,
            tree_size: proof.tree_size,
            rekor_root_hex: hex_encode(&proof.rekor_root),
            proof_hashes_hex: proof.hashes.iter().map(|hash| hex_encode(hash)).collect(),
            checkpoint: proof.checkpoint,
            signature_b64: proof.signature_b64,
            integrated_time_ms,
        };
        verify_anchor(batch, &receipt, &self.hmac_key, &self.origin)?;
        Ok(receipt)
    }

    fn submit_root(&self, root: &[u8; 32]) -> Result<ParsedProof, AnchorError> {
        let body = serde_json::json!({
            "hashedRekordRequestV002": {
                "digest": b64_encode(root),
                "signature": {
                    "content": b64_encode(root),
                    "verifier": {
                        "keyDetails": "AF_TRACE_MERKLE_SHA256",
                        "publicKey": { "rawBytes": "" }
                    }
                }
            }
        });
        let bytes =
            serde_json::to_vec(&body).map_err(|err| AnchorError::Malformed(err.to_string()))?;
        let response = http_util::post_json(&self.url, &bytes, self.timeout)
            .map_err(AnchorError::Unreachable)?;
        if !(200..300).contains(&response.status) {
            let snippet = String::from_utf8_lossy(&response.body);
            let snippet = snippet.chars().take(180).collect::<String>();
            return Err(AnchorError::Unreachable(format!(
                "HTTP {} {snippet}",
                response.status
            )));
        }
        parse_proof_json(&response.body)
    }
}

struct ParsedProof {
    log_index: u64,
    tree_size: u64,
    rekor_root: [u8; 32],
    hashes: Vec<[u8; 32]>,
    checkpoint: String,
    signature_b64: String,
}

pub(crate) fn event_commitment(event: &TraceEvent) -> Vec<u8> {
    serde_json::to_vec(event).expect("TraceEvent 可以序列化成 JSON")
}

fn batch_root(batch: &[TraceEvent]) -> Result<[u8; 32], AnchorError> {
    let owned: Vec<Vec<u8>> = batch.iter().map(event_commitment).collect();
    let refs: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
    merkle_root(&refs).ok_or(AnchorError::EmptyBatch)
}

fn entries_url(endpoint: &str) -> Result<String, AnchorError> {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if !trimmed.starts_with("http://") {
        return Err(AnchorError::Endpoint(format!(
            "只支持 http:// 端点，实际 {endpoint}"
        )));
    }
    if trimmed.ends_with("/api/v2/log/entries") {
        Ok(trimmed.to_string())
    } else {
        Ok(format!("{trimmed}/api/v2/log/entries"))
    }
}

fn parse_proof_json(body: &[u8]) -> Result<ParsedProof, AnchorError> {
    let value: serde_json::Value = serde_json::from_slice(body)
        .map_err(|err| AnchorError::Malformed(format!("响应不是 JSON: {err}")))?;
    let log_index = json_u64(value.get("logIndex"))
        .ok_or_else(|| AnchorError::Malformed("缺少 logIndex".into()))?;
    let proof = value
        .get("inclusionProof")
        .ok_or_else(|| AnchorError::Malformed("缺少 inclusionProof".into()))?;
    let tree_size = json_u64(proof.get("treeSize"))
        .ok_or_else(|| AnchorError::Malformed("缺少 treeSize".into()))?;
    let root_b64 = proof
        .get("rootHash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AnchorError::Malformed("缺少 rootHash".into()))?;
    let rekor_root = decode_b64_hash(root_b64, "rootHash")?;
    let hash_values = proof
        .get("hashes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AnchorError::Malformed("缺少 hashes".into()))?;
    let mut hashes = Vec::with_capacity(hash_values.len());
    for (idx, hash) in hash_values.iter().enumerate() {
        let text = hash
            .as_str()
            .ok_or_else(|| AnchorError::Malformed(format!("hashes[{idx}] 不是字符串")))?;
        hashes.push(decode_b64_hash(text, "hashes")?);
    }
    let checkpoint = proof
        .get("checkpoint")
        .and_then(|v| v.get("envelope"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| AnchorError::Malformed("缺少 checkpoint.envelope".into()))?
        .to_string();
    let signature_b64 = signature_from_checkpoint(&checkpoint)?;
    Ok(ParsedProof {
        log_index,
        tree_size,
        rekor_root,
        hashes,
        checkpoint,
        signature_b64,
    })
}

fn json_u64(value: Option<&serde_json::Value>) -> Option<u64> {
    match value? {
        serde_json::Value::String(text) => text.parse().ok(),
        serde_json::Value::Number(number) => number.as_u64(),
        _ => None,
    }
}

fn decode_hash(hex: &str, field: &str) -> Result<[u8; 32], AnchorError> {
    let bytes =
        hex_decode(hex).ok_or_else(|| AnchorError::Malformed(format!("{field} 不是十六进制")))?;
    bytes
        .try_into()
        .map_err(|_| AnchorError::Malformed(format!("{field} 不是 32 字节")))
}

fn decode_b64_hash(text: &str, field: &str) -> Result<[u8; 32], AnchorError> {
    let bytes =
        b64_decode(text).ok_or_else(|| AnchorError::Malformed(format!("{field} 不是 base64")))?;
    bytes
        .try_into()
        .map_err(|_| AnchorError::Malformed(format!("{field} 不是 32 字节")))
}

fn decode_proof(hashes: &[String]) -> Result<Vec<[u8; 32]>, AnchorError> {
    hashes
        .iter()
        .map(|hash| decode_hash(hash, "proof"))
        .collect()
}

fn signature_from_checkpoint(envelope: &str) -> Result<String, AnchorError> {
    let (_, sig_part) = envelope
        .split_once("\n\n")
        .ok_or_else(|| AnchorError::Malformed("检查点缺少签名分隔".into()))?;
    let line = sig_part
        .lines()
        .next()
        .ok_or_else(|| AnchorError::Malformed("检查点缺少签名行".into()))?;
    let (_, sig_b64) = line
        .rsplit_once(' ')
        .ok_or_else(|| AnchorError::Malformed("签名行缺少空格".into()))?;
    if sig_b64.is_empty() || b64_decode(sig_b64).is_none() {
        return Err(AnchorError::Malformed("签名不是 base64".into()));
    }
    Ok(sig_b64.to_string())
}

fn verify_checkpoint(
    envelope: &str,
    origin: &str,
    hmac_key: &[u8],
    tree_size: u64,
    rekor_root: &[u8; 32],
    signature_b64: &str,
) -> Result<(), AnchorError> {
    let (body, sig_part) = envelope
        .split_once("\n\n")
        .ok_or_else(|| AnchorError::CheckpointMismatch("缺少空行".into()))?;
    let mut lines = body.lines();
    let got_origin = lines
        .next()
        .ok_or_else(|| AnchorError::CheckpointMismatch("缺少 origin".into()))?;
    let got_size: u64 = lines
        .next()
        .ok_or_else(|| AnchorError::CheckpointMismatch("缺少树大小".into()))?
        .parse()
        .map_err(|_| AnchorError::CheckpointMismatch("树大小不是整数".into()))?;
    let root_b64 = lines
        .next()
        .ok_or_else(|| AnchorError::CheckpointMismatch("缺少树根".into()))?;
    if got_origin != origin {
        return Err(AnchorError::CheckpointMismatch(format!(
            "origin={got_origin}，期望 {origin}"
        )));
    }
    if got_size != tree_size {
        return Err(AnchorError::CheckpointMismatch(format!(
            "tree_size={got_size}，回执 {tree_size}"
        )));
    }
    let root = decode_b64_hash(root_b64, "checkpoint root")?;
    if root != *rekor_root {
        return Err(AnchorError::CheckpointMismatch("树根与回执不一致".into()));
    }
    let line = sig_part
        .lines()
        .next()
        .ok_or_else(|| AnchorError::CheckpointMismatch("缺少签名行".into()))?;
    let prefix = format!("— {origin} ");
    let sig_b64 = line
        .strip_prefix(&prefix)
        .ok_or_else(|| AnchorError::CheckpointMismatch(format!("签名行不是「{prefix}」开头")))?;
    if sig_b64 != signature_b64 {
        return Err(AnchorError::BadSignature);
    }
    let sig = b64_decode(sig_b64).ok_or(AnchorError::BadSignature)?;
    let note = format!("{body}\n");
    let expect = hmac_sha256(hmac_key, note.as_bytes());
    if !ct_eq(&sig, &expect) {
        return Err(AnchorError::BadSignature);
    }
    Ok(())
}

/// 测试用的内存日志。叶子规则与 [`verify_anchor`] 相同，检查点用同一把 HMAC 密钥。
#[cfg(test)]
pub(crate) struct MemoryLog {
    leaves: Vec<[u8; 32]>,
    origin: String,
    key: Vec<u8>,
}

#[cfg(test)]
impl MemoryLog {
    pub(crate) fn new(origin: &str, key: &[u8]) -> Self {
        Self {
            leaves: Vec::new(),
            origin: origin.to_string(),
            key: key.to_vec(),
        }
    }

    pub(crate) fn append(&mut self, batch_root: &[u8; 32]) -> AnchorReceipt {
        let leaf = leaf_hash(batch_root);
        self.leaves.push(leaf);
        let index = self.leaves.len() - 1;
        let tree_root = root_from_leaf_hashes(&self.leaves);
        let proof = inclusion_proof(&self.leaves, index).expect("刚追加的下标有效");
        let (checkpoint, signature_b64) = sign_checkpoint(
            &self.origin,
            self.leaves.len() as u64,
            &tree_root,
            &self.key,
        );
        AnchorReceipt {
            batch_seqs: Vec::new(),
            merkle_root_hex: hex_encode(batch_root),
            log_index: u64::try_from(index).unwrap_or(u64::MAX),
            tree_size: u64::try_from(self.leaves.len()).unwrap_or(u64::MAX),
            rekor_root_hex: hex_encode(&tree_root),
            proof_hashes_hex: proof.iter().map(|hash| hex_encode(hash)).collect(),
            checkpoint,
            signature_b64,
            integrated_time_ms: 0,
        }
    }

    pub(crate) fn proof_json(&self, receipt: &AnchorReceipt) -> Vec<u8> {
        let root = decode_hash(&receipt.rekor_root_hex, "rekor_root_hex").expect("根");
        let hashes: Vec<String> = receipt
            .proof_hashes_hex
            .iter()
            .map(|hex| {
                let bytes = decode_hash(hex, "proof").expect("证明");
                b64_encode(&bytes)
            })
            .collect();
        serde_json::to_vec(&serde_json::json!({
            "logIndex": receipt.log_index.to_string(),
            "inclusionProof": {
                "logIndex": receipt.log_index.to_string(),
                "treeSize": receipt.tree_size.to_string(),
                "rootHash": b64_encode(&root),
                "hashes": hashes,
                "checkpoint": { "envelope": receipt.checkpoint }
            }
        }))
        .expect("证明 JSON")
    }
}

#[cfg(test)]
pub(crate) fn sign_checkpoint(
    origin: &str,
    tree_size: u64,
    root: &[u8; 32],
    key: &[u8],
) -> (String, String) {
    let note = format!("{origin}\n{tree_size}\n{}\n", b64_encode(root));
    let sig = hmac_sha256(key, note.as_bytes());
    let signature_b64 = b64_encode(&sig);
    let envelope = format!("{note}\n— {origin} {signature_b64}\n");
    (envelope, signature_b64)
}

#[cfg(test)]
pub(crate) mod stub {
    use super::*;
    use crate::sha256::b64_decode;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::Duration;

    pub(crate) struct RekorStub {
        addr: std::net::SocketAddr,
        stop: Arc<AtomicBool>,
        join: Option<JoinHandle<()>>,
        pub fail_remaining: Arc<AtomicU64>,
        pub accepted: Arc<AtomicU64>,
        pub entered: Arc<AtomicU64>,
        pub gate: Arc<Mutex<()>>,
        pub paths: Arc<Mutex<Vec<String>>>,
    }

    impl RekorStub {
        pub(crate) fn spawn(origin: &str, key: &[u8]) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("绑定 stub 端口");
            let addr = listener.local_addr().expect("stub 地址");
            listener.set_nonblocking(false).expect("stub 阻塞接受");
            let stop = Arc::new(AtomicBool::new(false));
            let fail_remaining = Arc::new(AtomicU64::new(0));
            let accepted = Arc::new(AtomicU64::new(0));
            let entered = Arc::new(AtomicU64::new(0));
            let gate = Arc::new(Mutex::new(()));
            let paths = Arc::new(Mutex::new(Vec::new()));
            let log = Arc::new(Mutex::new(MemoryLog::new(origin, key)));
            let shared = Shared {
                stop: Arc::clone(&stop),
                fail_remaining: Arc::clone(&fail_remaining),
                accepted: Arc::clone(&accepted),
                entered: Arc::clone(&entered),
                gate: Arc::clone(&gate),
                paths: Arc::clone(&paths),
                log,
            };
            let join = std::thread::Builder::new()
                .name("af-trace-rekor-stub".into())
                .spawn(move || serve(listener, shared))
                .expect("启动 rekor stub");
            Self {
                addr,
                stop,
                join: Some(join),
                fail_remaining,
                accepted,
                entered,
                gate,
                paths,
            }
        }

        pub(crate) fn endpoint(&self) -> String {
            format!("http://{}", self.addr)
        }
    }

    impl Drop for RekorStub {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(500));
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    struct Shared {
        stop: Arc<AtomicBool>,
        fail_remaining: Arc<AtomicU64>,
        accepted: Arc<AtomicU64>,
        entered: Arc<AtomicU64>,
        gate: Arc<Mutex<()>>,
        paths: Arc<Mutex<Vec<String>>>,
        log: Arc<Mutex<MemoryLog>>,
    }

    fn serve(listener: TcpListener, shared: Shared) {
        let _ = listener.set_nonblocking(false);
        while !shared.stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    if shared.stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let _ = handle_one(stream, &shared);
                }
                Err(_) => break,
            }
        }
    }

    fn handle_one(mut stream: TcpStream, shared: &Shared) -> std::io::Result<()> {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
        let request = match read_request(&mut stream) {
            Ok(request) => request,
            Err(_) => return Ok(()),
        };
        if request.method != "POST" || request.path.split('?').next() != Some("/api/v2/log/entries")
        {
            return write_json(&mut stream, 404, br#"{"error":"not found"}"#);
        }
        shared.paths.lock().expect("paths").push(request.path);
        shared.entered.fetch_add(1, Ordering::SeqCst);
        let _hold = shared.gate.lock().expect("gate");
        if shared
            .fail_remaining
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            })
            .is_ok()
        {
            // 读完请求后直接关掉连接，模拟 rekor 不可达。
            drop(stream);
            return Ok(());
        }
        let digest = match digest_from_body(&request.body) {
            Ok(digest) => digest,
            Err(err) => {
                let body = serde_json::json!({ "error": err }).to_string();
                return write_json(&mut stream, 400, body.as_bytes());
            }
        };
        let receipt = {
            let mut guard = shared.log.lock().expect("log");
            guard.append(&digest)
        };
        shared.accepted.fetch_add(1, Ordering::SeqCst);
        let body = shared.log.lock().expect("log").proof_json(&receipt);
        write_json(&mut stream, 201, &body)
    }

    struct Request {
        method: String,
        path: String,
        body: Vec<u8>,
    }

    fn read_request(stream: &mut TcpStream) -> std::io::Result<Request> {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 2048];
        let header_end = loop {
            let n = stream.read(&mut tmp)?;
            if n == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "请求中断",
                ));
            }
            buf.extend_from_slice(&tmp[..n]);
            if let Some(pos) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
                break pos;
            }
            if buf.len() > 1024 * 1024 {
                return Err(std::io::Error::other("请求头过大"));
            }
        };
        let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
        let mut lines = head.lines();
        let request_line = lines.next().unwrap_or_default();
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let path = parts.next().unwrap_or("").to_string();
        let mut length = 0usize;
        for line in lines {
            if let Some((name, value)) = line.split_once(':') {
                if name.eq_ignore_ascii_case("content-length") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
        }
        let mut body = buf[header_end + 4..].to_vec();
        while body.len() < length {
            let n = stream.read(&mut tmp)?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&tmp[..n]);
        }
        body.truncate(length);
        Ok(Request { method, path, body })
    }

    fn digest_from_body(body: &[u8]) -> Result<[u8; 32], String> {
        let value: serde_json::Value =
            serde_json::from_slice(body).map_err(|err| format!("请求不是 JSON: {err}"))?;
        let digest = value
            .get("hashedRekordRequestV002")
            .and_then(|v| v.get("digest"))
            .and_then(|v| v.as_str())
            .ok_or("缺少 digest")?;
        let bytes = b64_decode(digest).ok_or("digest 不是 base64")?;
        bytes
            .try_into()
            .map_err(|_| "digest 不是 32 字节".to_string())
    }

    fn write_json(stream: &mut TcpStream, status: u16, body: &[u8]) -> std::io::Result<()> {
        let reason = if status == 201 { "Created" } else { "Error" };
        let header = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(header.as_bytes())?;
        stream.write_all(body)?;
        stream.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Level, TraceEvent};
    use serde_json::json;

    fn event(seq: u64, kind: &str) -> TraceEvent {
        TraceEvent {
            seq,
            level: Level::Critical,
            kind: kind.to_string(),
            payload: json!({ "seq": seq }),
            ts_ms: 1_700_000_000_000 + seq,
            gap: None,
        }
    }

    fn sample_batch() -> Vec<TraceEvent> {
        vec![event(1, "audit"), event(2, "audit"), event(4, "gap")]
    }

    #[test]
    fn memory_log_receipt_verifies_and_rejects_tampering() {
        let key = b"test-rekor-key";
        let origin = "rekor-local";
        let batch = sample_batch();
        let root = batch_root(&batch).unwrap();
        let mut log = MemoryLog::new(origin, key);
        let mut receipt = log.append(&root);
        receipt.batch_seqs = batch.iter().map(|event| event.seq).collect();
        receipt.integrated_time_ms = 42;
        verify_anchor(&batch, &receipt, key, origin).unwrap();

        let parsed = parse_proof_json(&log.proof_json(&receipt)).unwrap();
        assert_eq!(parsed.log_index, receipt.log_index);
        assert_eq!(parsed.tree_size, receipt.tree_size);
        assert_eq!(hex_encode(&parsed.rekor_root), receipt.rekor_root_hex);
        assert_eq!(parsed.signature_b64, receipt.signature_b64);

        let mut bad_batch = batch.clone();
        bad_batch[0].payload = json!({ "seq": 99 });
        assert_eq!(
            verify_anchor(&bad_batch, &receipt, key, origin).unwrap_err(),
            AnchorError::RootMismatch
        );

        let mut bad_proof = receipt.clone();
        if bad_proof.proof_hashes_hex.is_empty() {
            bad_proof.tree_size += 1;
        } else {
            bad_proof.proof_hashes_hex[0] = hex_encode(&[0u8; 32]);
        }
        assert!(verify_anchor(&batch, &bad_proof, key, origin).is_err());

        let mut bad_sig = receipt.clone();
        bad_sig.signature_b64 = b64_encode(&[7u8; 32]);
        let (body, _) = bad_sig.checkpoint.split_once("\n\n").unwrap();
        bad_sig.checkpoint = format!("{body}\n\n— {origin} {}\n", bad_sig.signature_b64);
        assert_eq!(
            verify_anchor(&batch, &bad_sig, key, origin).unwrap_err(),
            AnchorError::BadSignature
        );

        assert_eq!(
            verify_anchor(&batch, &receipt, b"other-key", origin).unwrap_err(),
            AnchorError::BadSignature
        );
        assert!(matches!(
            verify_anchor(&[], &receipt, key, origin).unwrap_err(),
            AnchorError::EmptyBatch
        ));
    }

    #[test]
    fn second_leaf_proof_covers_both_entries() {
        let key = b"k";
        let mut log = MemoryLog::new("rekor-local", key);
        let first = log.append(&[1u8; 32]);
        let second = log.append(&[2u8; 32]);
        assert_eq!(first.log_index, 0);
        assert!(first.proof_hashes_hex.is_empty());
        assert_eq!(first.tree_size, 1);
        assert_eq!(second.log_index, 1);
        assert_eq!(second.tree_size, 2);
        assert_eq!(second.proof_hashes_hex.len(), 1);
        let leaf = leaf_hash(&[2u8; 32]);
        let sibling = decode_hash(&second.proof_hashes_hex[0], "proof").unwrap();
        let root = decode_hash(&second.rekor_root_hex, "root").unwrap();
        assert_eq!(eval_inclusion(&leaf, 1, 2, &[sibling]).unwrap(), root);
    }

    #[test]
    fn http_stub_submits_and_connection_drop_is_unreachable() {
        let key = b"stub-key";
        let stub = stub::RekorStub::spawn("rekor-local", key);
        let client = RekorClient::new(&stub.endpoint(), "rekor-local", key).unwrap();
        let batch = sample_batch();
        let receipt = client.submit_batch(&batch, 99).unwrap();
        assert_eq!(receipt.integrated_time_ms, 99);
        assert_eq!(receipt.log_index, 0);
        assert!(stub.entered.load(std::sync::atomic::Ordering::SeqCst) >= 1);
        drop(stub.gate.lock().unwrap());
        assert_eq!(stub.accepted.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            stub.paths.lock().unwrap().as_slice(),
            ["/api/v2/log/entries".to_string()]
        );
        verify_anchor(&batch, &receipt, key, "rekor-local").unwrap();

        let later = vec![event(8, "audit")];
        let second = client.submit_batch(&later, 100).unwrap();
        assert_eq!(second.log_index, 1);
        assert_eq!(second.tree_size, 2);
        verify_anchor(&later, &second, key, "rekor-local").unwrap();
        verify_anchor(&batch, &receipt, key, "rekor-local").unwrap();

        stub.fail_remaining
            .store(1, std::sync::atomic::Ordering::SeqCst);
        let err = client.submit_batch(&later, 101).unwrap_err();
        assert!(
            matches!(err, AnchorError::Unreachable(_)),
            "断连应是不可达，实际 {err:?}"
        );
        assert_eq!(stub.accepted.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn refused_port_is_unreachable() {
        let client = RekorClient::new("http://127.0.0.1:1", "rekor-local", b"k").unwrap();
        let err = client.submit_batch(&sample_batch(), 1).unwrap_err();
        assert!(matches!(err, AnchorError::Unreachable(_)), "{err:?}");
    }
}
