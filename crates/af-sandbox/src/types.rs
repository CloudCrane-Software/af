//! 调用方注入的镜像、资源限额、命令和产物路径。限额不在本 crate 里写死。

use crate::error::{Result, SandboxError};

/// 容器镜像引用。空串和空白都拒绝，不替调用方修剪。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    reference: String,
}

impl Image {
    pub fn new(reference: impl Into<String>) -> Result<Self> {
        let reference = reference.into();
        if reference.is_empty() || reference.chars().any(char::is_whitespace) {
            return Err(SandboxError::InvalidImage);
        }
        Ok(Self { reference })
    }

    pub fn reference(&self) -> &str {
        &self.reference
    }
}

/// 一次会话的资源限额。三个数都由调用方注入，构造时拒绝 0。
///
/// `timeout_ms` 是硬上限：会话从打开时刻起到达该毫秒数就不再 exec；
/// 后端报告的单次耗时到达该毫秒数也视为超时。比较使用注入时钟，不睡眠。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resources {
    cpu_millicores: u64,
    memory_bytes: u64,
    timeout_ms: u64,
}

impl Resources {
    pub fn new(cpu_millicores: u64, memory_bytes: u64, timeout_ms: u64) -> Result<Self> {
        if cpu_millicores == 0 {
            return Err(SandboxError::InvalidResources {
                field: "cpu_millicores",
            });
        }
        if memory_bytes == 0 {
            return Err(SandboxError::InvalidResources {
                field: "memory_bytes",
            });
        }
        if timeout_ms == 0 {
            return Err(SandboxError::InvalidResources {
                field: "timeout_ms",
            });
        }
        Ok(Self {
            cpu_millicores,
            memory_bytes,
            timeout_ms,
        })
    }

    pub fn cpu_millicores(&self) -> u64 {
        self.cpu_millicores
    }

    pub fn memory_bytes(&self) -> u64 {
        self.memory_bytes
    }

    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }
}

/// 在沙箱里执行的命令。空参数列表或空参数都拒绝。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    argv: Vec<String>,
}

impl Command {
    pub fn new(argv: impl IntoIterator<Item = impl Into<String>>) -> Result<Self> {
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
        if argv.is_empty() || argv.iter().any(String::is_empty) {
            return Err(SandboxError::InvalidCommand);
        }
        Ok(Self { argv })
    }

    pub fn argv(&self) -> &[String] {
        &self.argv
    }
}

/// 要从会话里取出的路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactSpec {
    path: String,
}

impl ArtifactSpec {
    pub fn new(path: impl Into<String>) -> Result<Self> {
        let path = path.into();
        if path.is_empty() {
            return Err(SandboxError::InvalidArtifact { path });
        }
        Ok(Self { path })
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

/// 取出的产物。字节是后端给出的原文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_injection() {
        assert!(matches!(Image::new(""), Err(SandboxError::InvalidImage)));
        assert!(matches!(
            Image::new("bad ref"),
            Err(SandboxError::InvalidImage)
        ));
        assert!(matches!(
            Resources::new(0, 1, 1),
            Err(SandboxError::InvalidResources {
                field: "cpu_millicores"
            })
        ));
        assert!(matches!(
            Resources::new(1, 0, 1),
            Err(SandboxError::InvalidResources {
                field: "memory_bytes"
            })
        ));
        assert!(matches!(
            Resources::new(1, 1, 0),
            Err(SandboxError::InvalidResources {
                field: "timeout_ms"
            })
        ));
        assert!(matches!(
            Command::new(Vec::<String>::new()),
            Err(SandboxError::InvalidCommand)
        ));
        assert!(matches!(
            Command::new([""]),
            Err(SandboxError::InvalidCommand)
        ));
        assert!(matches!(
            ArtifactSpec::new(""),
            Err(SandboxError::InvalidArtifact { .. })
        ));
    }
}
