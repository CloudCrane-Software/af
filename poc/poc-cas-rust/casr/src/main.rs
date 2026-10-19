// casr — 最小 OCI distribution 客户端（纯 Rust，手写 wire 层：ureq + sha2）
// 目的：对本地 zot 实例闭环 工件 push/pull（blob 上传 + manifest PUT + digest 校验）
// 上传形态（OCI dist spec 谱系，全部可测）：
//   mono        POST init → PUT ?digest=<d>（body=File，Content-Length，磁盘流式）
//   single-post 单请求 POST ?digest=<d>（body 直接跟 POST，部分 registry 支持）
//   chunked-te  POST init → PATCH（Transfer-Encoding: chunked，流式，无 Content-Range）→ PUT finalize
//   chunks      POST init → N× PATCH（Content-Range: a-b，固定 buffer）→ PUT finalize（协议分块）
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

const OCI_MANIFEST_MT: &str = "application/vnd.oci.image.manifest.v1+json";
const OCI_INDEX_MT: &str = "application/vnd.oci.image.index.v1+json";

type Res<T> = Result<T, Box<dyn std::error::Error>>;

struct HttpError {
    status: u16,
    url: String,
    body: String,
}
impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP {} for {} body={}", self.status, self.url, self.body)
    }
}
impl std::fmt::Debug for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}
impl std::error::Error for HttpError {}

fn to_hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

fn sha256_bytes(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    to_hex(&h.finalize())
}

/// 流过 reader 计算 sha256 与字节数（恒定内存）
fn sha256_stream(r: &mut dyn Read) -> std::io::Result<(String, u64)> {
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        total += n as u64;
    }
    Ok((to_hex(&h.finalize()), total))
}

/// 简版 RFC3339（UTC，秒级）
fn rfc3339_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // Howard Hinnant civil_from_days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

struct Reg {
    agent: ureq::Agent,
    base: String,
}

impl Reg {
    fn new(base: &str) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false) // 非 2xx 也当正常响应，自己判
            .build()
            .new_agent();
        Reg {
            agent,
            base: base.trim_end_matches('/').to_string(),
        }
    }

    fn abs(&self, loc: &str) -> String {
        if loc.starts_with("http://") || loc.starts_with("https://") {
            loc.to_string()
        } else {
            format!("{}{}", self.base, loc)
        }
    }

    fn check(
        &self,
        mut res: ureq::http::Response<ureq::Body>,
        url: String,
        want: &[u16],
    ) -> Res<ureq::http::Response<ureq::Body>> {
        let st = res.status().as_u16();
        if want.contains(&st) {
            Ok(res)
        } else {
            let body = res.body_mut().read_to_string().unwrap_or_default();
            Err(Box::new(HttpError {
                status: st,
                url,
                body: body.chars().take(500).collect(),
            }))
        }
    }

    fn head_blob(&self, repo: &str, digest: &str) -> Res<Option<u64>> {
        let url = self.abs(&format!("/v2/{repo}/blobs/{digest}"));
        let res = self.agent.head(&url).call()?;
        match res.status().as_u16() {
            200 => Ok(Some(
                res.headers()
                    .get("Content-Length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
            )),
            404 => Ok(None),
            s => Err(Box::new(HttpError {
                status: s,
                url,
                body: String::new(),
            })),
        }
    }

    fn init_upload(&self, repo: &str) -> Res<String> {
        let url = self.abs(&format!("/v2/{repo}/blobs/uploads/"));
        let res = self.agent.post(&url).send(ureq::SendBody::none())?;
        let res = self.check(res, url, &[202])?;
        let loc = res
            .headers()
            .get("Location")
            .and_then(|v| v.to_str().ok())
            .ok_or("202 without Location")?
            .to_string();
        Ok(loc)
    }

    /// PUT finalize：空 body，带 digest 参数
    fn finalize_upload(&self, loc: &str, digest: &str) -> Res<()> {
        let sep = if loc.contains('?') { '&' } else { '?' };
        let url = self.abs(&format!("{loc}{sep}digest={digest}"));
        let res = self.agent.put(&url).send(ureq::SendBody::none())?;
        self.check(res, url, &[201])?;
        Ok(())
    }

    /// 形态 mono：POST init → PUT（body=File → Content-Length，磁盘流式）
    fn put_blob_mono(&self, repo: &str, path: &str, digest: &str) -> Res<()> {
        let loc = self.init_upload(repo)?;
        let sep = if loc.contains('?') { '&' } else { '?' };
        let url = self.abs(&format!("{loc}{sep}digest={digest}"));
        let file = File::open(path)?;
        let res = self
            .agent
            .put(&url)
            .header("Content-Type", "application/octet-stream")
            .send(file)?;
        self.check(res, url, &[201])?;
        Ok(())
    }

    /// 形态 single-post：单请求 POST ?digest=（探测性）
    fn put_blob_single_post(&self, repo: &str, path: &str, digest: &str) -> Res<u16> {
        let url = self.abs(&format!("/v2/{repo}/blobs/uploads/?digest={digest}"));
        let file = File::open(path)?;
        let res = self
            .agent
            .post(&url)
            .header("Content-Type", "application/octet-stream")
            .send(file)?;
        let st = res.status().as_u16();
        let mut res = res;
        let _ = res.body_mut().read_to_string();
        Ok(st)
    }

    /// 形态 chunked-te：PATCH 用 Transfer-Encoding: chunked 流式（ureq from_reader，未知长度）
    fn put_blob_chunked_te(&self, repo: &str, path: &str, digest: &str) -> Res<()> {
        let loc = self.init_upload(repo)?;
        let url = self.abs(&loc);
        let mut file = BufReader::new(File::open(path)?);
        let res = self
            .agent
            .patch(&url)
            .header("Content-Type", "application/octet-stream")
            .send(ureq::SendBody::from_reader(&mut file))?;
        let res = self.check(res, url, &[202])?;
        if let Some(range) = res.headers().get("Range").and_then(|v| v.to_str().ok()) {
            eprintln!("  [chunked-te] PATCH 202 Range={range}");
        }
        self.finalize_upload(&loc, digest)
    }

    /// 形态 chunks：N× PATCH（Content-Range: a-b，固定 chunk_size 内存 buffer）
    fn put_blob_chunks(&self, repo: &str, path: &str, digest: &str, chunk_size: usize) -> Res<()> {
        let loc = self.init_upload(repo)?;
        let mut file = File::open(path)?;
        let mut buf = vec![0u8; chunk_size];
        let mut off = 0u64;
        loop {
            let n = read_full(&mut file, &mut buf)?;
            if n == 0 {
                break;
            }
            let url = self.abs(&loc);
            let res = self
                .agent
                .patch(&url)
                .header("Content-Type", "application/octet-stream")
                .header(
                    "Content-Range",
                    &format!("{}-{}", off, off + n as u64 - 1),
                )
                .send(&buf[..n])?;
            let res = self.check(res, url, &[202])?;
            let range = res
                .headers()
                .get("Range")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            eprintln!("  [chunks] PATCH off={off} n={n} -> 202 Range={range}");
            off += n as u64;
        }
        self.finalize_upload(&loc, digest)
    }

    fn put_manifest(&self, repo: &str, reference: &str, body: &[u8], ct: &str) -> Res<String> {
        let url = self.abs(&format!("/v2/{repo}/manifests/{reference}"));
        let res = self.agent.put(&url).header("Content-Type", ct).send(body)?;
        let res = self.check(res, url, &[201])?;
        let dcd = res
            .headers()
            .get("Docker-Content-Digest")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let mut res = res;
        let _ = res.body_mut().read_to_string();
        Ok(dcd)
    }

    fn get_manifest(&self, repo: &str, reference: &str) -> Res<(Vec<u8>, String)> {
        let url = self.abs(&format!("/v2/{repo}/manifests/{reference}"));
        let res = self
            .agent
            .get(&url)
            .header("Accept", &format!("{OCI_MANIFEST_MT}, {OCI_INDEX_MT}"))
            .call()?;
        let res = self.check(res, url, &[200])?;
        let ct = res
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let dcd = res
            .headers()
            .get("Docker-Content-Digest")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let mut res = res;
        let body = res.body_mut().read_to_vec()?;
        let local = sha256_bytes(&body);
        eprintln!("  [get-manifest] ct={ct} server-digest={dcd} local=sha256:{local}");
        if !dcd.is_empty() && dcd != format!("sha256:{local}") {
            return Err(format!(
                "manifest digest mismatch: server={dcd} local=sha256:{local}"
            )
            .into());
        }
        Ok((
            body,
            if dcd.is_empty() {
                format!("sha256:{local}")
            } else {
                dcd
            },
        ))
    }

    fn get_blob(&self, repo: &str, digest: &str, out_path: &str) -> Res<(String, u64)> {
        let url = self.abs(&format!("/v2/{repo}/blobs/{digest}"));
        let res = self.agent.get(&url).call()?;
        let res = self.check(res, url, &[200])?;
        let mut reader = res.into_body().into_reader();
        let mut out = File::create(out_path)?;
        let mut h = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            h.update(&buf[..n]);
            total += n as u64;
        }
        out.flush()?;
        Ok((format!("sha256:{}", to_hex(&h.finalize())), total))
    }

    fn get_referrers(
        &self,
        repo: &str,
        digest: &str,
        artifact_type: Option<&str>,
    ) -> Res<(u16, String, String)> {
        let mut url = self.abs(&format!("/v2/{repo}/referrers/{digest}"));
        if let Some(at) = artifact_type {
            url.push_str(&format!("?artifactType={}", urlencode(at)));
        }
        let res = self
            .agent
            .get(&url)
            .header("Accept", &format!("{OCI_INDEX_MT}, {OCI_MANIFEST_MT}"))
            .call()?;
        let st = res.status().as_u16();
        let ct = res
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let mut res = res;
        let body = res.body_mut().read_to_string()?;
        Ok((st, ct, body))
    }
}

fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'+' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn usage() -> &'static str {
    "用法:
  casr push <base> <repo> <tag> <content-file> [--form mono|single-post|chunked-te|chunks] [--chunk-size N] [--artifact-type MT] [--layer-mt MT] [--config-mt MT]
  casr pull <base> <repo> <tag> <out-dir>
  casr blob-push <base> <repo> <file> --form mono|single-post|chunked-te|chunks [--chunk-size N]
  casr blob-pull <base> <repo> <digest> <out-file>
  casr refs <base> <repo> <digest> [artifactType]"
}

fn main() -> Res<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("{}", usage());
        std::process::exit(2);
    }
    match args[0].as_str() {
        "push" => cmd_push(&args),
        "pull" => cmd_pull(&args),
        "blob-push" => cmd_blob_push(&args),
        "blob-pull" => cmd_blob_pull(&args),
        "refs" => cmd_refs(&args),
        _ => {
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    }
}

struct Common {
    form: String,
    chunk_size: usize,
    artifact_type: String,
    layer_mt: String,
    config_mt: String,
    /// 确定性模式：config/manifest 去掉一切时间戳与 name/tag 等外部性字段 → 同内容必得同 manifest digest
    deterministic: bool,
}

fn parse_common(args: &[String], idx0: usize) -> Common {
    let mut c = Common {
        form: "mono".into(),
        chunk_size: 8 * 1024 * 1024,
        artifact_type: "application/vnd.af.artifact.v1+json".into(),
        layer_mt: "application/vnd.af.artifact.layer.v1.raw".into(),
        config_mt: "application/vnd.af.artifact.config.v1+json".into(),
        deterministic: false,
    };
    let mut i = idx0;
    while i < args.len() {
        match args[i].as_str() {
            "--form" => { c.form = args[i + 1].clone(); i += 2; }
            "--chunk-size" => { c.chunk_size = args[i + 1].parse().unwrap(); i += 2; }
            "--artifact-type" => { c.artifact_type = args[i + 1].clone(); i += 2; }
            "--layer-mt" => { c.layer_mt = args[i + 1].clone(); i += 2; }
            "--config-mt" => { c.config_mt = args[i + 1].clone(); i += 2; }
            "--deterministic" => { c.deterministic = true; i += 1; }
            _ => { i += 1; }
        }
    }
    c
}

fn upload_blob(reg: &Reg, repo: &str, path: &str, digest: &str, c: &Common) -> Res<()> {
    let t0 = Instant::now();
    match c.form.as_str() {
        "mono" => reg.put_blob_mono(repo, path, digest)?,
        "single-post" => {
            let st = reg.put_blob_single_post(repo, path, digest)?;
            if st != 201 && st != 202 {
                return Err(format!("single-post got HTTP {st}").into());
            }
        }
        "chunked-te" => reg.put_blob_chunked_te(repo, path, digest)?,
        "chunks" => reg.put_blob_chunks(repo, path, digest, c.chunk_size)?,
        other => return Err(format!("unknown form: {other}").into()),
    }
    eprintln!("  [upload] form={} {} in {:?}", c.form, digest, t0.elapsed());
    Ok(())
}

fn cmd_push(args: &[String]) -> Res<()> {
    if args.len() < 5 {
        eprintln!("{}", usage());
        std::process::exit(2);
    }
    let (base, repo, tag, content) = (&args[1], &args[2], &args[3], &args[4]);
    let c = parse_common(args, 5);
    let reg = Reg::new(base);

    // 1) 内容 layer：流式算 digest/size
    let t0 = Instant::now();
    let mut f = File::open(content)?;
    let (layer_hex, layer_size) = sha256_stream(&mut f)?;
    let layer_digest = format!("sha256:{layer_hex}");
    eprintln!(
        "  [layer] {layer_digest} size={layer_size} (hash {:?})",
        t0.elapsed()
    );

    // 2) config blob（元数据；deterministic 模式只保留内容内在字段）
    let mut cfg = serde_json::json!({
        "artifactType": c.artifact_type,
        "created": rfc3339_now(),
        "name": repo,
        "tag": tag,
        "contentSize": layer_size,
        "contentDigest": layer_digest,
        "annotations": { "made-by": "casr-poc" },
    });
    if c.deterministic {
        cfg = serde_json::json!({
            "artifactType": c.artifact_type,
            "contentSize": layer_size,
            "contentDigest": layer_digest,
        });
    }
    let cfg_bytes = serde_json::to_vec_pretty(&cfg)?;
    let config_digest = format!("sha256:{}", sha256_bytes(&cfg_bytes));
    let cfg_tmp = format!("{content}.casr-config.json");
    std::fs::write(&cfg_tmp, &cfg_bytes)?;
    eprintln!("  [config] {config_digest} size={}", cfg_bytes.len());

    // 3) 上传两个 blob（config 用 mono；layer 用指定形态）
    if reg.head_blob(repo, &config_digest)?.is_some() {
        eprintln!("  [config] blob already exists, skip upload");
    } else {
        let c_mono = Common { form: "mono".into(), ..c.clone() };
        upload_blob(&reg, repo, &cfg_tmp, &config_digest, &c_mono)?;
    }
    upload_blob(&reg, repo, content, &layer_digest, &c)?;

    // 4) manifest PUT
    let mut manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": OCI_MANIFEST_MT,
        "artifactType": c.artifact_type,
        "config": { "mediaType": c.config_mt, "digest": config_digest, "size": cfg_bytes.len() },
        "layers": [ { "mediaType": c.layer_mt, "digest": layer_digest, "size": layer_size } ],
        "annotations": { "org.opencontainers.image.created": rfc3339_now() },
    });
    if c.deterministic {
        manifest.as_object_mut().unwrap().remove("annotations");
    }
    let man_bytes = serde_json::to_vec_pretty(&manifest)?;
    let man_digest = reg.put_manifest(repo, tag, &man_bytes, OCI_MANIFEST_MT)?;
    let local_man = format!("sha256:{}", sha256_bytes(&man_bytes));
    eprintln!("  [manifest] server={man_digest} local={local_man}");
    if !man_digest.is_empty() && man_digest != local_man {
        return Err("manifest digest mismatch after PUT".into());
    }
    println!(
        "{}",
        serde_json::json!({
            "manifest": if man_digest.is_empty() { local_man.clone() } else { man_digest },
            "config": config_digest,
            "layer": layer_digest,
            "layerSize": layer_size,
            "manifestSize": man_bytes.len(),
        })
    );
    std::fs::remove_file(&cfg_tmp).ok();
    Ok(())
}

fn cmd_pull(args: &[String]) -> Res<()> {
    if args.len() < 5 {
        eprintln!("{}", usage());
        std::process::exit(2);
    }
    let (base, repo, tag, out_dir) = (&args[1], &args[2], &args[3], &args[4]);
    std::fs::create_dir_all(out_dir)?;
    let reg = Reg::new(base);
    let t0 = Instant::now();

    // 1) manifest（含 digest 自校验）
    let (man_bytes, man_digest) = reg.get_manifest(repo, tag)?;
    std::fs::write(format!("{out_dir}/manifest.json"), &man_bytes)?;
    let man: serde_json::Value = serde_json::from_slice(&man_bytes)?;

    // 2) config blob
    let cd = man["config"]["digest"]
        .as_str()
        .ok_or("manifest missing config.digest")?;
    let (cfg_got, cfg_len) = reg.get_blob(repo, cd, &format!("{out_dir}/config.json"))?;
    if cfg_got != cd {
        return Err(format!("config digest mismatch: want={cd} got={cfg_got}").into());
    }
    if cfg_len != man["config"]["size"].as_u64().unwrap_or(0) {
        return Err(format!(
            "config size mismatch: want={} got={cfg_len}",
            man["config"]["size"]
        )
        .into());
    }

    // 3) 每个 layer blob
    let layers = man["layers"].as_array().ok_or("manifest missing layers")?;
    let mut pulled = Vec::new();
    for (i, l) in layers.iter().enumerate() {
        let d = l["digest"].as_str().ok_or("layer missing digest")?;
        let out = if layers.len() == 1 {
            format!("{out_dir}/content.bin")
        } else {
            format!("{out_dir}/content-{i}.bin")
        };
        let t1 = Instant::now();
        let (got, len) = reg.get_blob(repo, d, &out)?;
        let dt = t1.elapsed();
        if got != d {
            return Err(format!("layer digest mismatch: want={d} got={got}").into());
        }
        if len != l["size"].as_u64().unwrap_or(0) {
            return Err(format!(
                "layer size mismatch: want={} got={len}",
                l["size"]
            )
            .into());
        }
        pulled.push(format!("{d} ({len}B, {dt:?})"));
    }
    println!(
        "{}",
        serde_json::json!({
            "manifest": man_digest,
            "config": cd,
            "layers": pulled,
            "totalElapsed": format!("{:?}", t0.elapsed()),
        })
    );
    Ok(())
}

impl Clone for Common {
    fn clone(&self) -> Self {
        Common {
            form: self.form.clone(),
            chunk_size: self.chunk_size,
            artifact_type: self.artifact_type.clone(),
            layer_mt: self.layer_mt.clone(),
            config_mt: self.config_mt.clone(),
            deterministic: self.deterministic,
        }
    }
}

fn cmd_blob_push(args: &[String]) -> Res<()> {
    if args.len() < 4 {
        eprintln!("{}", usage());
        std::process::exit(2);
    }
    let (base, repo, path) = (&args[1], &args[2], &args[3]);
    let c = parse_common(args, 4);
    let reg = Reg::new(base);
    let t0 = Instant::now();
    let mut f = File::open(path)?;
    let (hex, size) = sha256_stream(&mut f)?;
    let digest = format!("sha256:{hex}");
    eprintln!("  [hash] {digest} size={size} in {:?}", t0.elapsed());
    upload_blob(&reg, repo, path, &digest, &c)?;
    println!("{{\"digest\":\"{digest}\",\"size\":{size}}}");
    Ok(())
}

fn cmd_blob_pull(args: &[String]) -> Res<()> {
    if args.len() < 5 {
        eprintln!("{}", usage());
        std::process::exit(2);
    }
    let (base, repo, digest, out) = (&args[1], &args[2], &args[3], &args[4]);
    let reg = Reg::new(base);
    let t0 = Instant::now();
    let (got, len) = reg.get_blob(repo, digest, out)?;
    if got.as_str() != digest {
        return Err(format!("digest mismatch: want={digest} got={got}").into());
    }
    println!(
        "{{\"digest\":\"{got}\",\"size\":{len},\"elapsed\":\"{:?}\"}}",
        t0.elapsed()
    );
    Ok(())
}

fn cmd_refs(args: &[String]) -> Res<()> {
    if args.len() < 4 {
        eprintln!("{}", usage());
        std::process::exit(2);
    }
    let (base, repo, digest) = (&args[1], &args[2], &args[3]);
    let at = args.get(4).map(|s| s.as_str());
    let reg = Reg::new(base);
    let (st, ct, body) = reg.get_referrers(repo, digest, at)?;
    println!("HTTP {st}  Content-Type: {ct}");
    println!("{body}");
    Ok(())
}
