//! `engine_version` 与 `engine_version_min` 的 semver 比较。
//!
//! 规则与 Cargo 包版本一致：`MAJOR.MINOR.PATCH`，可选预发布标识与 build 元数据。
//! build 元数据不参与比较。预发布版本低于同号正式版。数字标识按整数比较。
//! 任一侧解析失败都是错误，调用方必须拒绝，不能把看不懂的下限当成没有下限。

#[derive(Debug, PartialEq, Eq)]
pub(super) enum VersionGate {
    /// 引擎版本低于调用方给出的下限。
    TooOld,
    /// `engine_version_min` 不是合法 semver。
    BadMin(&'static str),
    /// 编进二进制的版本字符串本身无法解析。
    BadEngine,
}

#[derive(Debug, PartialEq, Eq)]
struct SemVer {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Vec<PreIdent>,
}

/// 数字标识的优先级低于非数字标识，与 semver 规范一致。
/// 因此枚举声明顺序就是比较顺序，不要对调。
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum PreIdent {
    Num(u64),
    Text(String),
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => std::cmp::Ordering::Equal,
                // 正式版高于同号预发布版。空的 pre 更大。
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            })
    }
}

/// `Ok(())` 表示 `engine >= min`。
pub(super) fn check(engine: &str, min: &str) -> Result<(), VersionGate> {
    let engine = parse_semver(engine).map_err(|_| VersionGate::BadEngine)?;
    let min = parse_semver(min).map_err(VersionGate::BadMin)?;
    if engine < min {
        Err(VersionGate::TooOld)
    } else {
        Ok(())
    }
}

fn parse_semver(raw: &str) -> Result<SemVer, &'static str> {
    if raw.is_empty() {
        return Err("空字符串");
    }
    let (without_build, build) = match raw.split_once('+') {
        Some((version, build)) => (version, Some(build)),
        None => (raw, None),
    };
    if let Some(build) = build {
        parse_build(build)?;
    }
    let (core, pre) = match without_build.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (without_build, None),
    };
    let (major, minor, patch) = parse_core(core)?;
    let pre = match pre {
        Some(pre) => parse_pre(pre)?,
        None => Vec::new(),
    };
    Ok(SemVer {
        major,
        minor,
        patch,
        pre,
    })
}

fn parse_core(core: &str) -> Result<(u64, u64, u64), &'static str> {
    let mut parts = core.split('.');
    let major = parse_numeric_component(parts.next().ok_or("缺少主版本")?)?;
    let minor = parse_numeric_component(parts.next().ok_or("缺少次版本")?)?;
    let patch = parse_numeric_component(parts.next().ok_or("缺少修订号")?)?;
    if parts.next().is_some() {
        return Err("版本号超过三段");
    }
    Ok((major, minor, patch))
}

fn parse_numeric_component(raw: &str) -> Result<u64, &'static str> {
    if raw.is_empty() {
        return Err("版本段为空");
    }
    if !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err("版本段不是数字");
    }
    if raw.len() > 1 && raw.starts_with('0') {
        return Err("版本段有前导零");
    }
    raw.parse::<u64>().map_err(|_| "版本段超出范围")
}

fn parse_build(raw: &str) -> Result<(), &'static str> {
    if raw.is_empty() {
        return Err("build 元数据为空");
    }
    for part in raw.split('.') {
        if !is_ident_body(part) {
            return Err("build 元数据非法");
        }
    }
    Ok(())
}

fn parse_pre(raw: &str) -> Result<Vec<PreIdent>, &'static str> {
    if raw.is_empty() {
        return Err("预发布标识为空");
    }
    let mut out = Vec::new();
    for part in raw.split('.') {
        if !is_ident_body(part) {
            return Err("预发布标识非法");
        }
        if part.bytes().all(|b| b.is_ascii_digit()) {
            if part.len() > 1 && part.starts_with('0') {
                return Err("预发布数字有前导零");
            }
            let n = part.parse::<u64>().map_err(|_| "预发布数字超出范围")?;
            out.push(PreIdent::Num(n));
        } else {
            out.push(PreIdent::Text(part.to_owned()));
        }
    }
    Ok(out)
}

fn is_ident_body(raw: &str) -> bool {
    !raw.is_empty() && raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::{check, parse_semver, VersionGate};

    #[test]
    fn semver_precedence_is_strict_along_the_spec_chain() {
        let chain = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ];
        for window in chain.windows(2) {
            assert_eq!(
                check(window[1], window[0]),
                Ok(()),
                "{} 应不低于 {}",
                window[1],
                window[0]
            );
            assert_eq!(
                check(window[0], window[1]),
                Err(VersionGate::TooOld),
                "{} 应低于 {}",
                window[0],
                window[1]
            );
        }
        assert_eq!(check("1.0.0", "1.0.0"), Ok(()));
        assert_eq!(check("2.0.0", "1.9.9"), Ok(()));
        assert_eq!(check("1.9.9", "2.0.0"), Err(VersionGate::TooOld));
        assert_eq!(check("1.1.0", "1.0.9"), Ok(()));
        assert_eq!(check("1.0.9", "1.1.0"), Err(VersionGate::TooOld));
        assert_eq!(check("1.0.1", "1.0.0"), Ok(()));
        assert_eq!(check("1.0.0", "1.0.1"), Err(VersionGate::TooOld));
    }

    #[test]
    fn build_metadata_does_not_change_precedence() {
        assert_eq!(check("1.0.0+aaa", "1.0.0+bbb"), Ok(()));
        assert_eq!(check("1.0.0", "1.0.0+bbb"), Ok(()));
        assert_eq!(check("1.0.0+bbb", "1.0.0"), Ok(()));
        assert_eq!(check("1.0.0-rc.1+build.001", "1.0.0-rc.1"), Ok(()));
        assert_eq!(check("1.0.0", "1.0.0-rc.1+build"), Ok(()));
        assert_eq!(check("1.0.0-rc.1+build", "1.0.0"), Err(VersionGate::TooOld));
        assert_eq!(check("0.1.0+001", "0.1.0"), Ok(()));
    }

    #[test]
    fn equal_version_satisfies_min() {
        assert_eq!(check("0.1.0", "0.1.0"), Ok(()));
        assert_eq!(check("0.1.0", "0.0.0"), Ok(()));
        assert_eq!(check("0.1.0", "0.1.0-rc.1"), Ok(()));
        assert_eq!(check("0.1.0-rc.1", "0.1.0"), Err(VersionGate::TooOld));
        assert_eq!(check("0.1.0", "0.1.1"), Err(VersionGate::TooOld));
    }

    #[test]
    fn unparseable_versions_fail_closed() {
        let min_errors = [
            ("", "空字符串"),
            ("1", "缺少次版本"),
            ("1.0", "缺少修订号"),
            ("1.2.3.4", "版本号超过三段"),
            ("01.0.0", "版本段有前导零"),
            ("1.01.0", "版本段有前导零"),
            ("1.0.01", "版本段有前导零"),
            ("v1.0.0", "版本段不是数字"),
            ("1.0.0-", "预发布标识为空"),
            ("1.0.0+", "build 元数据为空"),
            ("1.0.0-01", "预发布数字有前导零"),
            ("1.0.0-rc..1", "预发布标识非法"),
            ("1.0.0-α", "预发布标识非法"),
            ("1.0.0+build+", "build 元数据非法"),
            ("1.0.0 ", "版本段不是数字"),
        ];
        for (raw, reason) in min_errors {
            assert_eq!(
                parse_semver(raw),
                Err(reason),
                "解析 {raw:?} 的失败原因应稳定"
            );
            assert_eq!(check("0.1.0", raw), Err(VersionGate::BadMin(reason)));
        }
        assert_eq!(check("nope", "0.1.0"), Err(VersionGate::BadEngine));
        assert_eq!(check("1.0", "0.1.0"), Err(VersionGate::BadEngine));
        assert!(parse_semver("0.0.0").is_ok());
        assert!(parse_semver("0.1.0-0").is_ok());
        assert!(parse_semver("0.1.0-rc.1").is_ok());
        assert!(parse_semver("0.1.0+001").is_ok());
    }
}
