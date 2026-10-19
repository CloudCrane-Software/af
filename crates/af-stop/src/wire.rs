//! 对外 JSON 边界。
//!
//! 跨边界的文档都带整数版本字段 `v`。当前只接受 [`WIRE_VERSION`]（1）。
//! 缺少 `v`、`v` 不是整数、或 `v` 不是 1，都直接返回错误，不把文档当成当前版本读入。
//!
//! 领域类型本身作为 `body`。信封是 [`Versioned`]：`{"v":1,"body":...}`。
//! [`encode_wire`] / [`decode_wire`] 是读写这个信封的入口。

use crate::error::{Error, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// 当前对外文档版本。
pub const WIRE_VERSION: u32 = 1;

/// 带版本的对外文档。`v` 只能是 [`WIRE_VERSION`]。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Versioned<T> {
    pub v: u32,
    pub body: T,
}

impl<T> Versioned<T> {
    pub fn new(body: T) -> Self {
        Self {
            v: WIRE_VERSION,
            body,
        }
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Versioned<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match read_version(&value) {
            Ok(v) if v == WIRE_VERSION => {}
            Ok(found) => {
                return Err(serde::de::Error::custom(format!(
                    "unknown version {found}, expected {WIRE_VERSION}"
                )));
            }
            Err(Error::MissingVersion) => {
                return Err(serde::de::Error::custom("missing version"));
            }
            Err(Error::UnreadableVersion { raw }) => {
                return Err(serde::de::Error::custom(format!(
                    "unreadable version {raw}"
                )));
            }
            Err(other) => {
                return Err(serde::de::Error::custom(other.to_string()));
            }
        }
        #[derive(Deserialize)]
        struct Raw<T> {
            v: u32,
            body: T,
        }
        let raw: Raw<T> = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Versioned {
            v: raw.v,
            body: raw.body,
        })
    }
}

/// 把领域值包进 `v = 1` 的信封。
pub fn encode_wire<T: Serialize>(body: &T) -> Result<String> {
    encode_doc(&Versioned::new(body))
}

/// 读信封并取出 `body`。版本不对时不会去解析 `body`。
pub fn decode_wire<T: DeserializeOwned>(text: &str) -> Result<T> {
    let wrapped: Versioned<T> = decode_doc(text)?;
    Ok(wrapped.body)
}

/// 写出已经自带 `v` 的文档。`v` 不是 1 则拒绝，避免把未知版本写进账本。
pub fn encode_doc<T: Serialize>(doc: &T) -> Result<String> {
    let value = serde_json::to_value(doc)?;
    ensure_version(&value)?;
    Ok(serde_json::to_string(&value)?)
}

/// 读自带 `v` 的文档。先看版本，版本不对就不反序列化其余字段。
pub fn decode_doc<T: DeserializeOwned>(text: &str) -> Result<T> {
    let value: Value = serde_json::from_str(text)?;
    ensure_version(&value)?;
    Ok(serde_json::from_value(value)?)
}

pub(crate) fn ensure_version(value: &Value) -> Result<u32> {
    let found = read_version(value)?;
    if found != WIRE_VERSION {
        return Err(Error::UnknownVersion {
            found,
            expected: WIRE_VERSION,
        });
    }
    Ok(found)
}

/// 反序列化结构体上的 `v` 字段。不是 1 就让 serde 失败。
pub(crate) fn require_v1<'de, D>(deserializer: D) -> std::result::Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match read_version_value(&value) {
        Ok(WIRE_VERSION) => Ok(WIRE_VERSION),
        Ok(found) => Err(serde::de::Error::custom(format!(
            "unknown version {found}, expected {WIRE_VERSION}"
        ))),
        Err(Error::UnreadableVersion { raw }) => Err(serde::de::Error::custom(format!(
            "unreadable version {raw}"
        ))),
        Err(other) => Err(serde::de::Error::custom(other.to_string())),
    }
}

fn read_version(value: &Value) -> Result<u32> {
    let Some(raw) = value.get("v") else {
        return Err(Error::MissingVersion);
    };
    read_version_value(raw)
}

fn read_version_value(raw: &Value) -> Result<u32> {
    match raw {
        Value::Number(number) if number.is_u64() => {
            let Some(wide) = number.as_u64() else {
                return Err(Error::UnreadableVersion {
                    raw: raw.to_string(),
                });
            };
            u32::try_from(wide).map_err(|_| Error::UnreadableVersion {
                raw: raw.to_string(),
            })
        }
        other => Err(Error::UnreadableVersion {
            raw: other.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Decision, Event, StopReason};

    #[test]
    fn wire_roundtrip_keeps_version_one() {
        let event = Event::ToolError {
            tool: "search".into(),
            error_code: "E_TIMEOUT".into(),
        };
        let text = encode_wire(&event).unwrap();
        assert!(text.contains("\"v\":1"), "{text}");
        let back: Event = decode_wire(&text).unwrap();
        assert_eq!(back, event);

        let decision = Decision::Stop {
            reason: StopReason::NoInfluence,
        };
        let encoded = encode_wire(&decision).unwrap();
        let decoded: Decision = decode_wire(&encoded).unwrap();
        assert_eq!(decoded, decision);
    }

    #[test]
    fn unknown_version_is_rejected_without_reading_the_body() {
        let text = r#"{"v":2,"body":{"not":"a real event"}}"#;
        let err = decode_wire::<Event>(text).unwrap_err();
        assert!(
            matches!(
                err,
                Error::UnknownVersion {
                    found: 2,
                    expected: 1
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn missing_or_unreadable_version_is_rejected() {
        let missing = decode_wire::<Event>(r#"{"body":"Continue"}"#).unwrap_err();
        assert!(matches!(missing, Error::MissingVersion), "{missing}");

        let text = decode_wire::<Event>(r#"{"v":"1","body":"Continue"}"#).unwrap_err();
        assert!(matches!(text, Error::UnreadableVersion { .. }), "{text}");

        let fraction = decode_wire::<Event>(r#"{"v":1.5,"body":"Continue"}"#).unwrap_err();
        assert!(
            matches!(fraction, Error::UnreadableVersion { .. }),
            "{fraction}"
        );

        let negative = decode_wire::<Event>(r#"{"v":-1,"body":"Continue"}"#).unwrap_err();
        assert!(
            matches!(negative, Error::UnreadableVersion { .. }),
            "{negative}"
        );

        let huge = decode_doc::<Versioned<Decision>>(&format!(
            r#"{{"v":{},"body":"Continue"}}"#,
            u64::from(u32::MAX) + 1
        ))
        .unwrap_err();
        assert!(matches!(huge, Error::UnreadableVersion { .. }), "{huge}");
    }

    #[test]
    fn version_one_with_a_bad_body_is_a_json_error() {
        let err = decode_wire::<Event>(r#"{"v":1,"body":{"nope":true}}"#).unwrap_err();
        assert!(matches!(err, Error::Json(_)), "{err}");
    }

    #[test]
    fn encode_refuses_to_write_an_unknown_version() {
        let doc = Versioned {
            v: 9,
            body: Decision::Continue,
        };
        let err = encode_doc(&doc).unwrap_err();
        assert!(
            matches!(
                err,
                Error::UnknownVersion {
                    found: 9,
                    expected: 1
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn direct_serde_of_the_envelope_also_rejects_unknown_versions() {
        let err = serde_json::from_str::<Versioned<Decision>>(r#"{"v":2,"body":"Continue"}"#)
            .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("unknown version 2"), "{message}");
    }
}
