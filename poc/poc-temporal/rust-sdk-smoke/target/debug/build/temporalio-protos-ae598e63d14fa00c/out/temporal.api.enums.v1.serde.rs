impl serde::Serialize for ActivityExecutionStatus {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "ACTIVITY_EXECUTION_STATUS_UNSPECIFIED",
            Self::Running => "ACTIVITY_EXECUTION_STATUS_RUNNING",
            Self::Completed => "ACTIVITY_EXECUTION_STATUS_COMPLETED",
            Self::Failed => "ACTIVITY_EXECUTION_STATUS_FAILED",
            Self::Canceled => "ACTIVITY_EXECUTION_STATUS_CANCELED",
            Self::Terminated => "ACTIVITY_EXECUTION_STATUS_TERMINATED",
            Self::TimedOut => "ACTIVITY_EXECUTION_STATUS_TIMED_OUT",
            Self::Paused => "ACTIVITY_EXECUTION_STATUS_PAUSED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ActivityExecutionStatus {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ACTIVITY_EXECUTION_STATUS_UNSPECIFIED",
            "ACTIVITY_EXECUTION_STATUS_RUNNING",
            "ACTIVITY_EXECUTION_STATUS_COMPLETED",
            "ACTIVITY_EXECUTION_STATUS_FAILED",
            "ACTIVITY_EXECUTION_STATUS_CANCELED",
            "ACTIVITY_EXECUTION_STATUS_TERMINATED",
            "ACTIVITY_EXECUTION_STATUS_TIMED_OUT",
            "ACTIVITY_EXECUTION_STATUS_PAUSED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ActivityExecutionStatus;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "ACTIVITY_EXECUTION_STATUS_UNSPECIFIED" => Ok(ActivityExecutionStatus::Unspecified),
                    "ACTIVITY_EXECUTION_STATUS_RUNNING" => Ok(ActivityExecutionStatus::Running),
                    "ACTIVITY_EXECUTION_STATUS_COMPLETED" => Ok(ActivityExecutionStatus::Completed),
                    "ACTIVITY_EXECUTION_STATUS_FAILED" => Ok(ActivityExecutionStatus::Failed),
                    "ACTIVITY_EXECUTION_STATUS_CANCELED" => Ok(ActivityExecutionStatus::Canceled),
                    "ACTIVITY_EXECUTION_STATUS_TERMINATED" => Ok(ActivityExecutionStatus::Terminated),
                    "ACTIVITY_EXECUTION_STATUS_TIMED_OUT" => Ok(ActivityExecutionStatus::TimedOut),
                    "ACTIVITY_EXECUTION_STATUS_PAUSED" => Ok(ActivityExecutionStatus::Paused),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ActivityIdConflictPolicy {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "ACTIVITY_ID_CONFLICT_POLICY_UNSPECIFIED",
            Self::Fail => "ACTIVITY_ID_CONFLICT_POLICY_FAIL",
            Self::UseExisting => "ACTIVITY_ID_CONFLICT_POLICY_USE_EXISTING",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ActivityIdConflictPolicy {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ACTIVITY_ID_CONFLICT_POLICY_UNSPECIFIED",
            "ACTIVITY_ID_CONFLICT_POLICY_FAIL",
            "ACTIVITY_ID_CONFLICT_POLICY_USE_EXISTING",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ActivityIdConflictPolicy;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "ACTIVITY_ID_CONFLICT_POLICY_UNSPECIFIED" => Ok(ActivityIdConflictPolicy::Unspecified),
                    "ACTIVITY_ID_CONFLICT_POLICY_FAIL" => Ok(ActivityIdConflictPolicy::Fail),
                    "ACTIVITY_ID_CONFLICT_POLICY_USE_EXISTING" => Ok(ActivityIdConflictPolicy::UseExisting),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ActivityIdReusePolicy {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "ACTIVITY_ID_REUSE_POLICY_UNSPECIFIED",
            Self::AllowDuplicate => "ACTIVITY_ID_REUSE_POLICY_ALLOW_DUPLICATE",
            Self::AllowDuplicateFailedOnly => "ACTIVITY_ID_REUSE_POLICY_ALLOW_DUPLICATE_FAILED_ONLY",
            Self::RejectDuplicate => "ACTIVITY_ID_REUSE_POLICY_REJECT_DUPLICATE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ActivityIdReusePolicy {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ACTIVITY_ID_REUSE_POLICY_UNSPECIFIED",
            "ACTIVITY_ID_REUSE_POLICY_ALLOW_DUPLICATE",
            "ACTIVITY_ID_REUSE_POLICY_ALLOW_DUPLICATE_FAILED_ONLY",
            "ACTIVITY_ID_REUSE_POLICY_REJECT_DUPLICATE",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ActivityIdReusePolicy;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "ACTIVITY_ID_REUSE_POLICY_UNSPECIFIED" => Ok(ActivityIdReusePolicy::Unspecified),
                    "ACTIVITY_ID_REUSE_POLICY_ALLOW_DUPLICATE" => Ok(ActivityIdReusePolicy::AllowDuplicate),
                    "ACTIVITY_ID_REUSE_POLICY_ALLOW_DUPLICATE_FAILED_ONLY" => Ok(ActivityIdReusePolicy::AllowDuplicateFailedOnly),
                    "ACTIVITY_ID_REUSE_POLICY_REJECT_DUPLICATE" => Ok(ActivityIdReusePolicy::RejectDuplicate),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ActivityTaskFailedCause {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "ACTIVITY_TASK_FAILED_CAUSE_UNSPECIFIED",
            Self::PayloadsTooLarge => "ACTIVITY_TASK_FAILED_CAUSE_PAYLOADS_TOO_LARGE",
            Self::ExternalStorageFailure => "ACTIVITY_TASK_FAILED_CAUSE_EXTERNAL_STORAGE_FAILURE",
            Self::ActivityWorkerUnhandledFailure => "ACTIVITY_TASK_FAILED_CAUSE_ACTIVITY_WORKER_UNHANDLED_FAILURE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ActivityTaskFailedCause {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ACTIVITY_TASK_FAILED_CAUSE_UNSPECIFIED",
            "ACTIVITY_TASK_FAILED_CAUSE_PAYLOADS_TOO_LARGE",
            "ACTIVITY_TASK_FAILED_CAUSE_EXTERNAL_STORAGE_FAILURE",
            "ACTIVITY_TASK_FAILED_CAUSE_ACTIVITY_WORKER_UNHANDLED_FAILURE",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ActivityTaskFailedCause;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "ACTIVITY_TASK_FAILED_CAUSE_UNSPECIFIED" => Ok(ActivityTaskFailedCause::Unspecified),
                    "ACTIVITY_TASK_FAILED_CAUSE_PAYLOADS_TOO_LARGE" => Ok(ActivityTaskFailedCause::PayloadsTooLarge),
                    "ACTIVITY_TASK_FAILED_CAUSE_EXTERNAL_STORAGE_FAILURE" => Ok(ActivityTaskFailedCause::ExternalStorageFailure),
                    "ACTIVITY_TASK_FAILED_CAUSE_ACTIVITY_WORKER_UNHANDLED_FAILURE" => Ok(ActivityTaskFailedCause::ActivityWorkerUnhandledFailure),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ApplicationErrorCategory {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "APPLICATION_ERROR_CATEGORY_UNSPECIFIED",
            Self::Benign => "APPLICATION_ERROR_CATEGORY_BENIGN",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ApplicationErrorCategory {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "APPLICATION_ERROR_CATEGORY_UNSPECIFIED",
            "APPLICATION_ERROR_CATEGORY_BENIGN",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ApplicationErrorCategory;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "APPLICATION_ERROR_CATEGORY_UNSPECIFIED" => Ok(ApplicationErrorCategory::Unspecified),
                    "APPLICATION_ERROR_CATEGORY_BENIGN" => Ok(ApplicationErrorCategory::Benign),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ArchivalState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "ARCHIVAL_STATE_UNSPECIFIED",
            Self::Disabled => "ARCHIVAL_STATE_DISABLED",
            Self::Enabled => "ARCHIVAL_STATE_ENABLED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ArchivalState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ARCHIVAL_STATE_UNSPECIFIED",
            "ARCHIVAL_STATE_DISABLED",
            "ARCHIVAL_STATE_ENABLED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ArchivalState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "ARCHIVAL_STATE_UNSPECIFIED" => Ok(ArchivalState::Unspecified),
                    "ARCHIVAL_STATE_DISABLED" => Ok(ArchivalState::Disabled),
                    "ARCHIVAL_STATE_ENABLED" => Ok(ArchivalState::Enabled),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for BatchOperationState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "BATCH_OPERATION_STATE_UNSPECIFIED",
            Self::Running => "BATCH_OPERATION_STATE_RUNNING",
            Self::Completed => "BATCH_OPERATION_STATE_COMPLETED",
            Self::Failed => "BATCH_OPERATION_STATE_FAILED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for BatchOperationState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "BATCH_OPERATION_STATE_UNSPECIFIED",
            "BATCH_OPERATION_STATE_RUNNING",
            "BATCH_OPERATION_STATE_COMPLETED",
            "BATCH_OPERATION_STATE_FAILED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = BatchOperationState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "BATCH_OPERATION_STATE_UNSPECIFIED" => Ok(BatchOperationState::Unspecified),
                    "BATCH_OPERATION_STATE_RUNNING" => Ok(BatchOperationState::Running),
                    "BATCH_OPERATION_STATE_COMPLETED" => Ok(BatchOperationState::Completed),
                    "BATCH_OPERATION_STATE_FAILED" => Ok(BatchOperationState::Failed),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for BatchOperationType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "BATCH_OPERATION_TYPE_UNSPECIFIED",
            Self::Terminate => "BATCH_OPERATION_TYPE_TERMINATE",
            Self::TerminateWorkflow => "BATCH_OPERATION_TYPE_TERMINATE_WORKFLOW",
            Self::Cancel => "BATCH_OPERATION_TYPE_CANCEL",
            Self::CancelWorkflow => "BATCH_OPERATION_TYPE_CANCEL_WORKFLOW",
            Self::Signal => "BATCH_OPERATION_TYPE_SIGNAL",
            Self::SignalWorkflow => "BATCH_OPERATION_TYPE_SIGNAL_WORKFLOW",
            Self::Delete => "BATCH_OPERATION_TYPE_DELETE",
            Self::DeleteWorkflow => "BATCH_OPERATION_TYPE_DELETE_WORKFLOW",
            Self::Reset => "BATCH_OPERATION_TYPE_RESET",
            Self::ResetWorkflow => "BATCH_OPERATION_TYPE_RESET_WORKFLOW",
            Self::UpdateExecutionOptions => "BATCH_OPERATION_TYPE_UPDATE_EXECUTION_OPTIONS",
            Self::UpdateWorkflowExecutionOptions => "BATCH_OPERATION_TYPE_UPDATE_WORKFLOW_EXECUTION_OPTIONS",
            Self::UnpauseActivity => "BATCH_OPERATION_TYPE_UNPAUSE_ACTIVITY",
            Self::UpdateActivityOptions => "BATCH_OPERATION_TYPE_UPDATE_ACTIVITY_OPTIONS",
            Self::ResetActivity => "BATCH_OPERATION_TYPE_RESET_ACTIVITY",
            Self::TerminateActivity => "BATCH_OPERATION_TYPE_TERMINATE_ACTIVITY",
            Self::CancelActivity => "BATCH_OPERATION_TYPE_CANCEL_ACTIVITY",
            Self::DeleteActivity => "BATCH_OPERATION_TYPE_DELETE_ACTIVITY",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for BatchOperationType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "BATCH_OPERATION_TYPE_UNSPECIFIED",
            "BATCH_OPERATION_TYPE_TERMINATE",
            "BATCH_OPERATION_TYPE_TERMINATE_WORKFLOW",
            "BATCH_OPERATION_TYPE_CANCEL",
            "BATCH_OPERATION_TYPE_CANCEL_WORKFLOW",
            "BATCH_OPERATION_TYPE_SIGNAL",
            "BATCH_OPERATION_TYPE_SIGNAL_WORKFLOW",
            "BATCH_OPERATION_TYPE_DELETE",
            "BATCH_OPERATION_TYPE_DELETE_WORKFLOW",
            "BATCH_OPERATION_TYPE_RESET",
            "BATCH_OPERATION_TYPE_RESET_WORKFLOW",
            "BATCH_OPERATION_TYPE_UPDATE_EXECUTION_OPTIONS",
            "BATCH_OPERATION_TYPE_UPDATE_WORKFLOW_EXECUTION_OPTIONS",
            "BATCH_OPERATION_TYPE_UNPAUSE_ACTIVITY",
            "BATCH_OPERATION_TYPE_UPDATE_ACTIVITY_OPTIONS",
            "BATCH_OPERATION_TYPE_RESET_ACTIVITY",
            "BATCH_OPERATION_TYPE_TERMINATE_ACTIVITY",
            "BATCH_OPERATION_TYPE_CANCEL_ACTIVITY",
            "BATCH_OPERATION_TYPE_DELETE_ACTIVITY",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = BatchOperationType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "BATCH_OPERATION_TYPE_UNSPECIFIED" => Ok(BatchOperationType::Unspecified),
                    "BATCH_OPERATION_TYPE_TERMINATE" => Ok(BatchOperationType::Terminate),
                    "BATCH_OPERATION_TYPE_TERMINATE_WORKFLOW" => Ok(BatchOperationType::TerminateWorkflow),
                    "BATCH_OPERATION_TYPE_CANCEL" => Ok(BatchOperationType::Cancel),
                    "BATCH_OPERATION_TYPE_CANCEL_WORKFLOW" => Ok(BatchOperationType::CancelWorkflow),
                    "BATCH_OPERATION_TYPE_SIGNAL" => Ok(BatchOperationType::Signal),
                    "BATCH_OPERATION_TYPE_SIGNAL_WORKFLOW" => Ok(BatchOperationType::SignalWorkflow),
                    "BATCH_OPERATION_TYPE_DELETE" => Ok(BatchOperationType::Delete),
                    "BATCH_OPERATION_TYPE_DELETE_WORKFLOW" => Ok(BatchOperationType::DeleteWorkflow),
                    "BATCH_OPERATION_TYPE_RESET" => Ok(BatchOperationType::Reset),
                    "BATCH_OPERATION_TYPE_RESET_WORKFLOW" => Ok(BatchOperationType::ResetWorkflow),
                    "BATCH_OPERATION_TYPE_UPDATE_EXECUTION_OPTIONS" => Ok(BatchOperationType::UpdateExecutionOptions),
                    "BATCH_OPERATION_TYPE_UPDATE_WORKFLOW_EXECUTION_OPTIONS" => Ok(BatchOperationType::UpdateWorkflowExecutionOptions),
                    "BATCH_OPERATION_TYPE_UNPAUSE_ACTIVITY" => Ok(BatchOperationType::UnpauseActivity),
                    "BATCH_OPERATION_TYPE_UPDATE_ACTIVITY_OPTIONS" => Ok(BatchOperationType::UpdateActivityOptions),
                    "BATCH_OPERATION_TYPE_RESET_ACTIVITY" => Ok(BatchOperationType::ResetActivity),
                    "BATCH_OPERATION_TYPE_TERMINATE_ACTIVITY" => Ok(BatchOperationType::TerminateActivity),
                    "BATCH_OPERATION_TYPE_CANCEL_ACTIVITY" => Ok(BatchOperationType::CancelActivity),
                    "BATCH_OPERATION_TYPE_DELETE_ACTIVITY" => Ok(BatchOperationType::DeleteActivity),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for BuildIdTaskReachability {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "BUILD_ID_TASK_REACHABILITY_UNSPECIFIED",
            Self::Reachable => "BUILD_ID_TASK_REACHABILITY_REACHABLE",
            Self::ClosedWorkflowsOnly => "BUILD_ID_TASK_REACHABILITY_CLOSED_WORKFLOWS_ONLY",
            Self::Unreachable => "BUILD_ID_TASK_REACHABILITY_UNREACHABLE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for BuildIdTaskReachability {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "BUILD_ID_TASK_REACHABILITY_UNSPECIFIED",
            "BUILD_ID_TASK_REACHABILITY_REACHABLE",
            "BUILD_ID_TASK_REACHABILITY_CLOSED_WORKFLOWS_ONLY",
            "BUILD_ID_TASK_REACHABILITY_UNREACHABLE",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = BuildIdTaskReachability;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "BUILD_ID_TASK_REACHABILITY_UNSPECIFIED" => Ok(BuildIdTaskReachability::Unspecified),
                    "BUILD_ID_TASK_REACHABILITY_REACHABLE" => Ok(BuildIdTaskReachability::Reachable),
                    "BUILD_ID_TASK_REACHABILITY_CLOSED_WORKFLOWS_ONLY" => Ok(BuildIdTaskReachability::ClosedWorkflowsOnly),
                    "BUILD_ID_TASK_REACHABILITY_UNREACHABLE" => Ok(BuildIdTaskReachability::Unreachable),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for CallbackState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "CALLBACK_STATE_UNSPECIFIED",
            Self::Standby => "CALLBACK_STATE_STANDBY",
            Self::Scheduled => "CALLBACK_STATE_SCHEDULED",
            Self::BackingOff => "CALLBACK_STATE_BACKING_OFF",
            Self::Failed => "CALLBACK_STATE_FAILED",
            Self::Succeeded => "CALLBACK_STATE_SUCCEEDED",
            Self::Blocked => "CALLBACK_STATE_BLOCKED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for CallbackState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "CALLBACK_STATE_UNSPECIFIED",
            "CALLBACK_STATE_STANDBY",
            "CALLBACK_STATE_SCHEDULED",
            "CALLBACK_STATE_BACKING_OFF",
            "CALLBACK_STATE_FAILED",
            "CALLBACK_STATE_SUCCEEDED",
            "CALLBACK_STATE_BLOCKED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = CallbackState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "CALLBACK_STATE_UNSPECIFIED" => Ok(CallbackState::Unspecified),
                    "CALLBACK_STATE_STANDBY" => Ok(CallbackState::Standby),
                    "CALLBACK_STATE_SCHEDULED" => Ok(CallbackState::Scheduled),
                    "CALLBACK_STATE_BACKING_OFF" => Ok(CallbackState::BackingOff),
                    "CALLBACK_STATE_FAILED" => Ok(CallbackState::Failed),
                    "CALLBACK_STATE_SUCCEEDED" => Ok(CallbackState::Succeeded),
                    "CALLBACK_STATE_BLOCKED" => Ok(CallbackState::Blocked),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for CancelExternalWorkflowExecutionFailedCause {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "CANCEL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_UNSPECIFIED",
            Self::ExternalWorkflowExecutionNotFound => "CANCEL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_EXTERNAL_WORKFLOW_EXECUTION_NOT_FOUND",
            Self::NamespaceNotFound => "CANCEL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_NAMESPACE_NOT_FOUND",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for CancelExternalWorkflowExecutionFailedCause {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "CANCEL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_UNSPECIFIED",
            "CANCEL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_EXTERNAL_WORKFLOW_EXECUTION_NOT_FOUND",
            "CANCEL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_NAMESPACE_NOT_FOUND",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = CancelExternalWorkflowExecutionFailedCause;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "CANCEL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_UNSPECIFIED" => Ok(CancelExternalWorkflowExecutionFailedCause::Unspecified),
                    "CANCEL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_EXTERNAL_WORKFLOW_EXECUTION_NOT_FOUND" => Ok(CancelExternalWorkflowExecutionFailedCause::ExternalWorkflowExecutionNotFound),
                    "CANCEL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_NAMESPACE_NOT_FOUND" => Ok(CancelExternalWorkflowExecutionFailedCause::NamespaceNotFound),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for CommandType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "COMMAND_TYPE_UNSPECIFIED",
            Self::ScheduleActivityTask => "COMMAND_TYPE_SCHEDULE_ACTIVITY_TASK",
            Self::RequestCancelActivityTask => "COMMAND_TYPE_REQUEST_CANCEL_ACTIVITY_TASK",
            Self::StartTimer => "COMMAND_TYPE_START_TIMER",
            Self::CompleteWorkflowExecution => "COMMAND_TYPE_COMPLETE_WORKFLOW_EXECUTION",
            Self::FailWorkflowExecution => "COMMAND_TYPE_FAIL_WORKFLOW_EXECUTION",
            Self::CancelTimer => "COMMAND_TYPE_CANCEL_TIMER",
            Self::CancelWorkflowExecution => "COMMAND_TYPE_CANCEL_WORKFLOW_EXECUTION",
            Self::RequestCancelExternalWorkflowExecution => "COMMAND_TYPE_REQUEST_CANCEL_EXTERNAL_WORKFLOW_EXECUTION",
            Self::RecordMarker => "COMMAND_TYPE_RECORD_MARKER",
            Self::ContinueAsNewWorkflowExecution => "COMMAND_TYPE_CONTINUE_AS_NEW_WORKFLOW_EXECUTION",
            Self::StartChildWorkflowExecution => "COMMAND_TYPE_START_CHILD_WORKFLOW_EXECUTION",
            Self::SignalExternalWorkflowExecution => "COMMAND_TYPE_SIGNAL_EXTERNAL_WORKFLOW_EXECUTION",
            Self::UpsertWorkflowSearchAttributes => "COMMAND_TYPE_UPSERT_WORKFLOW_SEARCH_ATTRIBUTES",
            Self::ProtocolMessage => "COMMAND_TYPE_PROTOCOL_MESSAGE",
            Self::ModifyWorkflowProperties => "COMMAND_TYPE_MODIFY_WORKFLOW_PROPERTIES",
            Self::ScheduleNexusOperation => "COMMAND_TYPE_SCHEDULE_NEXUS_OPERATION",
            Self::RequestCancelNexusOperation => "COMMAND_TYPE_REQUEST_CANCEL_NEXUS_OPERATION",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for CommandType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "COMMAND_TYPE_UNSPECIFIED",
            "COMMAND_TYPE_SCHEDULE_ACTIVITY_TASK",
            "COMMAND_TYPE_REQUEST_CANCEL_ACTIVITY_TASK",
            "COMMAND_TYPE_START_TIMER",
            "COMMAND_TYPE_COMPLETE_WORKFLOW_EXECUTION",
            "COMMAND_TYPE_FAIL_WORKFLOW_EXECUTION",
            "COMMAND_TYPE_CANCEL_TIMER",
            "COMMAND_TYPE_CANCEL_WORKFLOW_EXECUTION",
            "COMMAND_TYPE_REQUEST_CANCEL_EXTERNAL_WORKFLOW_EXECUTION",
            "COMMAND_TYPE_RECORD_MARKER",
            "COMMAND_TYPE_CONTINUE_AS_NEW_WORKFLOW_EXECUTION",
            "COMMAND_TYPE_START_CHILD_WORKFLOW_EXECUTION",
            "COMMAND_TYPE_SIGNAL_EXTERNAL_WORKFLOW_EXECUTION",
            "COMMAND_TYPE_UPSERT_WORKFLOW_SEARCH_ATTRIBUTES",
            "COMMAND_TYPE_PROTOCOL_MESSAGE",
            "COMMAND_TYPE_MODIFY_WORKFLOW_PROPERTIES",
            "COMMAND_TYPE_SCHEDULE_NEXUS_OPERATION",
            "COMMAND_TYPE_REQUEST_CANCEL_NEXUS_OPERATION",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = CommandType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "COMMAND_TYPE_UNSPECIFIED" => Ok(CommandType::Unspecified),
                    "COMMAND_TYPE_SCHEDULE_ACTIVITY_TASK" => Ok(CommandType::ScheduleActivityTask),
                    "COMMAND_TYPE_REQUEST_CANCEL_ACTIVITY_TASK" => Ok(CommandType::RequestCancelActivityTask),
                    "COMMAND_TYPE_START_TIMER" => Ok(CommandType::StartTimer),
                    "COMMAND_TYPE_COMPLETE_WORKFLOW_EXECUTION" => Ok(CommandType::CompleteWorkflowExecution),
                    "COMMAND_TYPE_FAIL_WORKFLOW_EXECUTION" => Ok(CommandType::FailWorkflowExecution),
                    "COMMAND_TYPE_CANCEL_TIMER" => Ok(CommandType::CancelTimer),
                    "COMMAND_TYPE_CANCEL_WORKFLOW_EXECUTION" => Ok(CommandType::CancelWorkflowExecution),
                    "COMMAND_TYPE_REQUEST_CANCEL_EXTERNAL_WORKFLOW_EXECUTION" => Ok(CommandType::RequestCancelExternalWorkflowExecution),
                    "COMMAND_TYPE_RECORD_MARKER" => Ok(CommandType::RecordMarker),
                    "COMMAND_TYPE_CONTINUE_AS_NEW_WORKFLOW_EXECUTION" => Ok(CommandType::ContinueAsNewWorkflowExecution),
                    "COMMAND_TYPE_START_CHILD_WORKFLOW_EXECUTION" => Ok(CommandType::StartChildWorkflowExecution),
                    "COMMAND_TYPE_SIGNAL_EXTERNAL_WORKFLOW_EXECUTION" => Ok(CommandType::SignalExternalWorkflowExecution),
                    "COMMAND_TYPE_UPSERT_WORKFLOW_SEARCH_ATTRIBUTES" => Ok(CommandType::UpsertWorkflowSearchAttributes),
                    "COMMAND_TYPE_PROTOCOL_MESSAGE" => Ok(CommandType::ProtocolMessage),
                    "COMMAND_TYPE_MODIFY_WORKFLOW_PROPERTIES" => Ok(CommandType::ModifyWorkflowProperties),
                    "COMMAND_TYPE_SCHEDULE_NEXUS_OPERATION" => Ok(CommandType::ScheduleNexusOperation),
                    "COMMAND_TYPE_REQUEST_CANCEL_NEXUS_OPERATION" => Ok(CommandType::RequestCancelNexusOperation),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ContinueAsNewInitiator {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "CONTINUE_AS_NEW_INITIATOR_UNSPECIFIED",
            Self::Workflow => "CONTINUE_AS_NEW_INITIATOR_WORKFLOW",
            Self::Retry => "CONTINUE_AS_NEW_INITIATOR_RETRY",
            Self::CronSchedule => "CONTINUE_AS_NEW_INITIATOR_CRON_SCHEDULE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ContinueAsNewInitiator {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "CONTINUE_AS_NEW_INITIATOR_UNSPECIFIED",
            "CONTINUE_AS_NEW_INITIATOR_WORKFLOW",
            "CONTINUE_AS_NEW_INITIATOR_RETRY",
            "CONTINUE_AS_NEW_INITIATOR_CRON_SCHEDULE",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ContinueAsNewInitiator;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "CONTINUE_AS_NEW_INITIATOR_UNSPECIFIED" => Ok(ContinueAsNewInitiator::Unspecified),
                    "CONTINUE_AS_NEW_INITIATOR_WORKFLOW" => Ok(ContinueAsNewInitiator::Workflow),
                    "CONTINUE_AS_NEW_INITIATOR_RETRY" => Ok(ContinueAsNewInitiator::Retry),
                    "CONTINUE_AS_NEW_INITIATOR_CRON_SCHEDULE" => Ok(ContinueAsNewInitiator::CronSchedule),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ContinueAsNewVersioningBehavior {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "CONTINUE_AS_NEW_VERSIONING_BEHAVIOR_UNSPECIFIED",
            Self::AutoUpgrade => "CONTINUE_AS_NEW_VERSIONING_BEHAVIOR_AUTO_UPGRADE",
            Self::UseRampingVersion => "CONTINUE_AS_NEW_VERSIONING_BEHAVIOR_USE_RAMPING_VERSION",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ContinueAsNewVersioningBehavior {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "CONTINUE_AS_NEW_VERSIONING_BEHAVIOR_UNSPECIFIED",
            "CONTINUE_AS_NEW_VERSIONING_BEHAVIOR_AUTO_UPGRADE",
            "CONTINUE_AS_NEW_VERSIONING_BEHAVIOR_USE_RAMPING_VERSION",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ContinueAsNewVersioningBehavior;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "CONTINUE_AS_NEW_VERSIONING_BEHAVIOR_UNSPECIFIED" => Ok(ContinueAsNewVersioningBehavior::Unspecified),
                    "CONTINUE_AS_NEW_VERSIONING_BEHAVIOR_AUTO_UPGRADE" => Ok(ContinueAsNewVersioningBehavior::AutoUpgrade),
                    "CONTINUE_AS_NEW_VERSIONING_BEHAVIOR_USE_RAMPING_VERSION" => Ok(ContinueAsNewVersioningBehavior::UseRampingVersion),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for DeploymentReachability {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "DEPLOYMENT_REACHABILITY_UNSPECIFIED",
            Self::Reachable => "DEPLOYMENT_REACHABILITY_REACHABLE",
            Self::ClosedWorkflowsOnly => "DEPLOYMENT_REACHABILITY_CLOSED_WORKFLOWS_ONLY",
            Self::Unreachable => "DEPLOYMENT_REACHABILITY_UNREACHABLE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for DeploymentReachability {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "DEPLOYMENT_REACHABILITY_UNSPECIFIED",
            "DEPLOYMENT_REACHABILITY_REACHABLE",
            "DEPLOYMENT_REACHABILITY_CLOSED_WORKFLOWS_ONLY",
            "DEPLOYMENT_REACHABILITY_UNREACHABLE",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = DeploymentReachability;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "DEPLOYMENT_REACHABILITY_UNSPECIFIED" => Ok(DeploymentReachability::Unspecified),
                    "DEPLOYMENT_REACHABILITY_REACHABLE" => Ok(DeploymentReachability::Reachable),
                    "DEPLOYMENT_REACHABILITY_CLOSED_WORKFLOWS_ONLY" => Ok(DeploymentReachability::ClosedWorkflowsOnly),
                    "DEPLOYMENT_REACHABILITY_UNREACHABLE" => Ok(DeploymentReachability::Unreachable),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for DescribeTaskQueueMode {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "DESCRIBE_TASK_QUEUE_MODE_UNSPECIFIED",
            Self::Enhanced => "DESCRIBE_TASK_QUEUE_MODE_ENHANCED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for DescribeTaskQueueMode {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "DESCRIBE_TASK_QUEUE_MODE_UNSPECIFIED",
            "DESCRIBE_TASK_QUEUE_MODE_ENHANCED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = DescribeTaskQueueMode;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "DESCRIBE_TASK_QUEUE_MODE_UNSPECIFIED" => Ok(DescribeTaskQueueMode::Unspecified),
                    "DESCRIBE_TASK_QUEUE_MODE_ENHANCED" => Ok(DescribeTaskQueueMode::Enhanced),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for EncodingType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "ENCODING_TYPE_UNSPECIFIED",
            Self::Proto3 => "ENCODING_TYPE_PROTO3",
            Self::Json => "ENCODING_TYPE_JSON",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for EncodingType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ENCODING_TYPE_UNSPECIFIED",
            "ENCODING_TYPE_PROTO3",
            "ENCODING_TYPE_JSON",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = EncodingType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "ENCODING_TYPE_UNSPECIFIED" => Ok(EncodingType::Unspecified),
                    "ENCODING_TYPE_PROTO3" => Ok(EncodingType::Proto3),
                    "ENCODING_TYPE_JSON" => Ok(EncodingType::Json),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for EventType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "EVENT_TYPE_UNSPECIFIED",
            Self::WorkflowExecutionStarted => "EVENT_TYPE_WORKFLOW_EXECUTION_STARTED",
            Self::WorkflowExecutionCompleted => "EVENT_TYPE_WORKFLOW_EXECUTION_COMPLETED",
            Self::WorkflowExecutionFailed => "EVENT_TYPE_WORKFLOW_EXECUTION_FAILED",
            Self::WorkflowExecutionTimedOut => "EVENT_TYPE_WORKFLOW_EXECUTION_TIMED_OUT",
            Self::WorkflowTaskScheduled => "EVENT_TYPE_WORKFLOW_TASK_SCHEDULED",
            Self::WorkflowTaskStarted => "EVENT_TYPE_WORKFLOW_TASK_STARTED",
            Self::WorkflowTaskCompleted => "EVENT_TYPE_WORKFLOW_TASK_COMPLETED",
            Self::WorkflowTaskTimedOut => "EVENT_TYPE_WORKFLOW_TASK_TIMED_OUT",
            Self::WorkflowTaskFailed => "EVENT_TYPE_WORKFLOW_TASK_FAILED",
            Self::ActivityTaskScheduled => "EVENT_TYPE_ACTIVITY_TASK_SCHEDULED",
            Self::ActivityTaskStarted => "EVENT_TYPE_ACTIVITY_TASK_STARTED",
            Self::ActivityTaskCompleted => "EVENT_TYPE_ACTIVITY_TASK_COMPLETED",
            Self::ActivityTaskFailed => "EVENT_TYPE_ACTIVITY_TASK_FAILED",
            Self::ActivityTaskTimedOut => "EVENT_TYPE_ACTIVITY_TASK_TIMED_OUT",
            Self::ActivityTaskCancelRequested => "EVENT_TYPE_ACTIVITY_TASK_CANCEL_REQUESTED",
            Self::ActivityTaskCanceled => "EVENT_TYPE_ACTIVITY_TASK_CANCELED",
            Self::TimerStarted => "EVENT_TYPE_TIMER_STARTED",
            Self::TimerFired => "EVENT_TYPE_TIMER_FIRED",
            Self::TimerCanceled => "EVENT_TYPE_TIMER_CANCELED",
            Self::WorkflowExecutionCancelRequested => "EVENT_TYPE_WORKFLOW_EXECUTION_CANCEL_REQUESTED",
            Self::WorkflowExecutionCanceled => "EVENT_TYPE_WORKFLOW_EXECUTION_CANCELED",
            Self::RequestCancelExternalWorkflowExecutionInitiated => "EVENT_TYPE_REQUEST_CANCEL_EXTERNAL_WORKFLOW_EXECUTION_INITIATED",
            Self::RequestCancelExternalWorkflowExecutionFailed => "EVENT_TYPE_REQUEST_CANCEL_EXTERNAL_WORKFLOW_EXECUTION_FAILED",
            Self::ExternalWorkflowExecutionCancelRequested => "EVENT_TYPE_EXTERNAL_WORKFLOW_EXECUTION_CANCEL_REQUESTED",
            Self::MarkerRecorded => "EVENT_TYPE_MARKER_RECORDED",
            Self::WorkflowExecutionSignaled => "EVENT_TYPE_WORKFLOW_EXECUTION_SIGNALED",
            Self::WorkflowExecutionTerminated => "EVENT_TYPE_WORKFLOW_EXECUTION_TERMINATED",
            Self::WorkflowExecutionContinuedAsNew => "EVENT_TYPE_WORKFLOW_EXECUTION_CONTINUED_AS_NEW",
            Self::StartChildWorkflowExecutionInitiated => "EVENT_TYPE_START_CHILD_WORKFLOW_EXECUTION_INITIATED",
            Self::StartChildWorkflowExecutionFailed => "EVENT_TYPE_START_CHILD_WORKFLOW_EXECUTION_FAILED",
            Self::ChildWorkflowExecutionStarted => "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_STARTED",
            Self::ChildWorkflowExecutionCompleted => "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_COMPLETED",
            Self::ChildWorkflowExecutionFailed => "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_FAILED",
            Self::ChildWorkflowExecutionCanceled => "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_CANCELED",
            Self::ChildWorkflowExecutionTimedOut => "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_TIMED_OUT",
            Self::ChildWorkflowExecutionTerminated => "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_TERMINATED",
            Self::SignalExternalWorkflowExecutionInitiated => "EVENT_TYPE_SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_INITIATED",
            Self::SignalExternalWorkflowExecutionFailed => "EVENT_TYPE_SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_FAILED",
            Self::ExternalWorkflowExecutionSignaled => "EVENT_TYPE_EXTERNAL_WORKFLOW_EXECUTION_SIGNALED",
            Self::UpsertWorkflowSearchAttributes => "EVENT_TYPE_UPSERT_WORKFLOW_SEARCH_ATTRIBUTES",
            Self::WorkflowExecutionUpdateAdmitted => "EVENT_TYPE_WORKFLOW_EXECUTION_UPDATE_ADMITTED",
            Self::WorkflowExecutionUpdateAccepted => "EVENT_TYPE_WORKFLOW_EXECUTION_UPDATE_ACCEPTED",
            Self::WorkflowExecutionUpdateRejected => "EVENT_TYPE_WORKFLOW_EXECUTION_UPDATE_REJECTED",
            Self::WorkflowExecutionUpdateCompleted => "EVENT_TYPE_WORKFLOW_EXECUTION_UPDATE_COMPLETED",
            Self::WorkflowPropertiesModifiedExternally => "EVENT_TYPE_WORKFLOW_PROPERTIES_MODIFIED_EXTERNALLY",
            Self::ActivityPropertiesModifiedExternally => "EVENT_TYPE_ACTIVITY_PROPERTIES_MODIFIED_EXTERNALLY",
            Self::WorkflowPropertiesModified => "EVENT_TYPE_WORKFLOW_PROPERTIES_MODIFIED",
            Self::NexusOperationScheduled => "EVENT_TYPE_NEXUS_OPERATION_SCHEDULED",
            Self::NexusOperationStarted => "EVENT_TYPE_NEXUS_OPERATION_STARTED",
            Self::NexusOperationCompleted => "EVENT_TYPE_NEXUS_OPERATION_COMPLETED",
            Self::NexusOperationFailed => "EVENT_TYPE_NEXUS_OPERATION_FAILED",
            Self::NexusOperationCanceled => "EVENT_TYPE_NEXUS_OPERATION_CANCELED",
            Self::NexusOperationTimedOut => "EVENT_TYPE_NEXUS_OPERATION_TIMED_OUT",
            Self::NexusOperationCancelRequested => "EVENT_TYPE_NEXUS_OPERATION_CANCEL_REQUESTED",
            Self::WorkflowExecutionOptionsUpdated => "EVENT_TYPE_WORKFLOW_EXECUTION_OPTIONS_UPDATED",
            Self::NexusOperationCancelRequestCompleted => "EVENT_TYPE_NEXUS_OPERATION_CANCEL_REQUEST_COMPLETED",
            Self::NexusOperationCancelRequestFailed => "EVENT_TYPE_NEXUS_OPERATION_CANCEL_REQUEST_FAILED",
            Self::WorkflowExecutionPaused => "EVENT_TYPE_WORKFLOW_EXECUTION_PAUSED",
            Self::WorkflowExecutionUnpaused => "EVENT_TYPE_WORKFLOW_EXECUTION_UNPAUSED",
            Self::WorkflowExecutionTimeSkippingTransitioned => "EVENT_TYPE_WORKFLOW_EXECUTION_TIME_SKIPPING_TRANSITIONED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for EventType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "EVENT_TYPE_UNSPECIFIED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_STARTED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_COMPLETED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_FAILED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_TIMED_OUT",
            "EVENT_TYPE_WORKFLOW_TASK_SCHEDULED",
            "EVENT_TYPE_WORKFLOW_TASK_STARTED",
            "EVENT_TYPE_WORKFLOW_TASK_COMPLETED",
            "EVENT_TYPE_WORKFLOW_TASK_TIMED_OUT",
            "EVENT_TYPE_WORKFLOW_TASK_FAILED",
            "EVENT_TYPE_ACTIVITY_TASK_SCHEDULED",
            "EVENT_TYPE_ACTIVITY_TASK_STARTED",
            "EVENT_TYPE_ACTIVITY_TASK_COMPLETED",
            "EVENT_TYPE_ACTIVITY_TASK_FAILED",
            "EVENT_TYPE_ACTIVITY_TASK_TIMED_OUT",
            "EVENT_TYPE_ACTIVITY_TASK_CANCEL_REQUESTED",
            "EVENT_TYPE_ACTIVITY_TASK_CANCELED",
            "EVENT_TYPE_TIMER_STARTED",
            "EVENT_TYPE_TIMER_FIRED",
            "EVENT_TYPE_TIMER_CANCELED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_CANCEL_REQUESTED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_CANCELED",
            "EVENT_TYPE_REQUEST_CANCEL_EXTERNAL_WORKFLOW_EXECUTION_INITIATED",
            "EVENT_TYPE_REQUEST_CANCEL_EXTERNAL_WORKFLOW_EXECUTION_FAILED",
            "EVENT_TYPE_EXTERNAL_WORKFLOW_EXECUTION_CANCEL_REQUESTED",
            "EVENT_TYPE_MARKER_RECORDED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_SIGNALED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_TERMINATED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_CONTINUED_AS_NEW",
            "EVENT_TYPE_START_CHILD_WORKFLOW_EXECUTION_INITIATED",
            "EVENT_TYPE_START_CHILD_WORKFLOW_EXECUTION_FAILED",
            "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_STARTED",
            "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_COMPLETED",
            "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_FAILED",
            "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_CANCELED",
            "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_TIMED_OUT",
            "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_TERMINATED",
            "EVENT_TYPE_SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_INITIATED",
            "EVENT_TYPE_SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_FAILED",
            "EVENT_TYPE_EXTERNAL_WORKFLOW_EXECUTION_SIGNALED",
            "EVENT_TYPE_UPSERT_WORKFLOW_SEARCH_ATTRIBUTES",
            "EVENT_TYPE_WORKFLOW_EXECUTION_UPDATE_ADMITTED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_UPDATE_ACCEPTED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_UPDATE_REJECTED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_UPDATE_COMPLETED",
            "EVENT_TYPE_WORKFLOW_PROPERTIES_MODIFIED_EXTERNALLY",
            "EVENT_TYPE_ACTIVITY_PROPERTIES_MODIFIED_EXTERNALLY",
            "EVENT_TYPE_WORKFLOW_PROPERTIES_MODIFIED",
            "EVENT_TYPE_NEXUS_OPERATION_SCHEDULED",
            "EVENT_TYPE_NEXUS_OPERATION_STARTED",
            "EVENT_TYPE_NEXUS_OPERATION_COMPLETED",
            "EVENT_TYPE_NEXUS_OPERATION_FAILED",
            "EVENT_TYPE_NEXUS_OPERATION_CANCELED",
            "EVENT_TYPE_NEXUS_OPERATION_TIMED_OUT",
            "EVENT_TYPE_NEXUS_OPERATION_CANCEL_REQUESTED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_OPTIONS_UPDATED",
            "EVENT_TYPE_NEXUS_OPERATION_CANCEL_REQUEST_COMPLETED",
            "EVENT_TYPE_NEXUS_OPERATION_CANCEL_REQUEST_FAILED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_PAUSED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_UNPAUSED",
            "EVENT_TYPE_WORKFLOW_EXECUTION_TIME_SKIPPING_TRANSITIONED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = EventType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "EVENT_TYPE_UNSPECIFIED" => Ok(EventType::Unspecified),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_STARTED" => Ok(EventType::WorkflowExecutionStarted),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_COMPLETED" => Ok(EventType::WorkflowExecutionCompleted),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_FAILED" => Ok(EventType::WorkflowExecutionFailed),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_TIMED_OUT" => Ok(EventType::WorkflowExecutionTimedOut),
                    "EVENT_TYPE_WORKFLOW_TASK_SCHEDULED" => Ok(EventType::WorkflowTaskScheduled),
                    "EVENT_TYPE_WORKFLOW_TASK_STARTED" => Ok(EventType::WorkflowTaskStarted),
                    "EVENT_TYPE_WORKFLOW_TASK_COMPLETED" => Ok(EventType::WorkflowTaskCompleted),
                    "EVENT_TYPE_WORKFLOW_TASK_TIMED_OUT" => Ok(EventType::WorkflowTaskTimedOut),
                    "EVENT_TYPE_WORKFLOW_TASK_FAILED" => Ok(EventType::WorkflowTaskFailed),
                    "EVENT_TYPE_ACTIVITY_TASK_SCHEDULED" => Ok(EventType::ActivityTaskScheduled),
                    "EVENT_TYPE_ACTIVITY_TASK_STARTED" => Ok(EventType::ActivityTaskStarted),
                    "EVENT_TYPE_ACTIVITY_TASK_COMPLETED" => Ok(EventType::ActivityTaskCompleted),
                    "EVENT_TYPE_ACTIVITY_TASK_FAILED" => Ok(EventType::ActivityTaskFailed),
                    "EVENT_TYPE_ACTIVITY_TASK_TIMED_OUT" => Ok(EventType::ActivityTaskTimedOut),
                    "EVENT_TYPE_ACTIVITY_TASK_CANCEL_REQUESTED" => Ok(EventType::ActivityTaskCancelRequested),
                    "EVENT_TYPE_ACTIVITY_TASK_CANCELED" => Ok(EventType::ActivityTaskCanceled),
                    "EVENT_TYPE_TIMER_STARTED" => Ok(EventType::TimerStarted),
                    "EVENT_TYPE_TIMER_FIRED" => Ok(EventType::TimerFired),
                    "EVENT_TYPE_TIMER_CANCELED" => Ok(EventType::TimerCanceled),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_CANCEL_REQUESTED" => Ok(EventType::WorkflowExecutionCancelRequested),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_CANCELED" => Ok(EventType::WorkflowExecutionCanceled),
                    "EVENT_TYPE_REQUEST_CANCEL_EXTERNAL_WORKFLOW_EXECUTION_INITIATED" => Ok(EventType::RequestCancelExternalWorkflowExecutionInitiated),
                    "EVENT_TYPE_REQUEST_CANCEL_EXTERNAL_WORKFLOW_EXECUTION_FAILED" => Ok(EventType::RequestCancelExternalWorkflowExecutionFailed),
                    "EVENT_TYPE_EXTERNAL_WORKFLOW_EXECUTION_CANCEL_REQUESTED" => Ok(EventType::ExternalWorkflowExecutionCancelRequested),
                    "EVENT_TYPE_MARKER_RECORDED" => Ok(EventType::MarkerRecorded),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_SIGNALED" => Ok(EventType::WorkflowExecutionSignaled),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_TERMINATED" => Ok(EventType::WorkflowExecutionTerminated),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_CONTINUED_AS_NEW" => Ok(EventType::WorkflowExecutionContinuedAsNew),
                    "EVENT_TYPE_START_CHILD_WORKFLOW_EXECUTION_INITIATED" => Ok(EventType::StartChildWorkflowExecutionInitiated),
                    "EVENT_TYPE_START_CHILD_WORKFLOW_EXECUTION_FAILED" => Ok(EventType::StartChildWorkflowExecutionFailed),
                    "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_STARTED" => Ok(EventType::ChildWorkflowExecutionStarted),
                    "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_COMPLETED" => Ok(EventType::ChildWorkflowExecutionCompleted),
                    "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_FAILED" => Ok(EventType::ChildWorkflowExecutionFailed),
                    "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_CANCELED" => Ok(EventType::ChildWorkflowExecutionCanceled),
                    "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_TIMED_OUT" => Ok(EventType::ChildWorkflowExecutionTimedOut),
                    "EVENT_TYPE_CHILD_WORKFLOW_EXECUTION_TERMINATED" => Ok(EventType::ChildWorkflowExecutionTerminated),
                    "EVENT_TYPE_SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_INITIATED" => Ok(EventType::SignalExternalWorkflowExecutionInitiated),
                    "EVENT_TYPE_SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_FAILED" => Ok(EventType::SignalExternalWorkflowExecutionFailed),
                    "EVENT_TYPE_EXTERNAL_WORKFLOW_EXECUTION_SIGNALED" => Ok(EventType::ExternalWorkflowExecutionSignaled),
                    "EVENT_TYPE_UPSERT_WORKFLOW_SEARCH_ATTRIBUTES" => Ok(EventType::UpsertWorkflowSearchAttributes),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_UPDATE_ADMITTED" => Ok(EventType::WorkflowExecutionUpdateAdmitted),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_UPDATE_ACCEPTED" => Ok(EventType::WorkflowExecutionUpdateAccepted),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_UPDATE_REJECTED" => Ok(EventType::WorkflowExecutionUpdateRejected),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_UPDATE_COMPLETED" => Ok(EventType::WorkflowExecutionUpdateCompleted),
                    "EVENT_TYPE_WORKFLOW_PROPERTIES_MODIFIED_EXTERNALLY" => Ok(EventType::WorkflowPropertiesModifiedExternally),
                    "EVENT_TYPE_ACTIVITY_PROPERTIES_MODIFIED_EXTERNALLY" => Ok(EventType::ActivityPropertiesModifiedExternally),
                    "EVENT_TYPE_WORKFLOW_PROPERTIES_MODIFIED" => Ok(EventType::WorkflowPropertiesModified),
                    "EVENT_TYPE_NEXUS_OPERATION_SCHEDULED" => Ok(EventType::NexusOperationScheduled),
                    "EVENT_TYPE_NEXUS_OPERATION_STARTED" => Ok(EventType::NexusOperationStarted),
                    "EVENT_TYPE_NEXUS_OPERATION_COMPLETED" => Ok(EventType::NexusOperationCompleted),
                    "EVENT_TYPE_NEXUS_OPERATION_FAILED" => Ok(EventType::NexusOperationFailed),
                    "EVENT_TYPE_NEXUS_OPERATION_CANCELED" => Ok(EventType::NexusOperationCanceled),
                    "EVENT_TYPE_NEXUS_OPERATION_TIMED_OUT" => Ok(EventType::NexusOperationTimedOut),
                    "EVENT_TYPE_NEXUS_OPERATION_CANCEL_REQUESTED" => Ok(EventType::NexusOperationCancelRequested),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_OPTIONS_UPDATED" => Ok(EventType::WorkflowExecutionOptionsUpdated),
                    "EVENT_TYPE_NEXUS_OPERATION_CANCEL_REQUEST_COMPLETED" => Ok(EventType::NexusOperationCancelRequestCompleted),
                    "EVENT_TYPE_NEXUS_OPERATION_CANCEL_REQUEST_FAILED" => Ok(EventType::NexusOperationCancelRequestFailed),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_PAUSED" => Ok(EventType::WorkflowExecutionPaused),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_UNPAUSED" => Ok(EventType::WorkflowExecutionUnpaused),
                    "EVENT_TYPE_WORKFLOW_EXECUTION_TIME_SKIPPING_TRANSITIONED" => Ok(EventType::WorkflowExecutionTimeSkippingTransitioned),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ExecutionType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "EXECUTION_TYPE_UNSPECIFIED",
            Self::Workflow => "EXECUTION_TYPE_WORKFLOW",
            Self::Activity => "EXECUTION_TYPE_ACTIVITY",
            Self::NexusOperation => "EXECUTION_TYPE_NEXUS_OPERATION",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ExecutionType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "EXECUTION_TYPE_UNSPECIFIED",
            "EXECUTION_TYPE_WORKFLOW",
            "EXECUTION_TYPE_ACTIVITY",
            "EXECUTION_TYPE_NEXUS_OPERATION",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ExecutionType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "EXECUTION_TYPE_UNSPECIFIED" => Ok(ExecutionType::Unspecified),
                    "EXECUTION_TYPE_WORKFLOW" => Ok(ExecutionType::Workflow),
                    "EXECUTION_TYPE_ACTIVITY" => Ok(ExecutionType::Activity),
                    "EXECUTION_TYPE_NEXUS_OPERATION" => Ok(ExecutionType::NexusOperation),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for FastForwardPollingResult {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "FAST_FORWARD_POLLING_RESULT_UNSPECIFIED",
            Self::PollTimeout => "FAST_FORWARD_POLLING_RESULT_POLL_TIMEOUT",
            Self::FastForwardCompleted => "FAST_FORWARD_POLLING_RESULT_FAST_FORWARD_COMPLETED",
            Self::FastForwardFailed => "FAST_FORWARD_POLLING_RESULT_FAST_FORWARD_FAILED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for FastForwardPollingResult {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "FAST_FORWARD_POLLING_RESULT_UNSPECIFIED",
            "FAST_FORWARD_POLLING_RESULT_POLL_TIMEOUT",
            "FAST_FORWARD_POLLING_RESULT_FAST_FORWARD_COMPLETED",
            "FAST_FORWARD_POLLING_RESULT_FAST_FORWARD_FAILED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = FastForwardPollingResult;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "FAST_FORWARD_POLLING_RESULT_UNSPECIFIED" => Ok(FastForwardPollingResult::Unspecified),
                    "FAST_FORWARD_POLLING_RESULT_POLL_TIMEOUT" => Ok(FastForwardPollingResult::PollTimeout),
                    "FAST_FORWARD_POLLING_RESULT_FAST_FORWARD_COMPLETED" => Ok(FastForwardPollingResult::FastForwardCompleted),
                    "FAST_FORWARD_POLLING_RESULT_FAST_FORWARD_FAILED" => Ok(FastForwardPollingResult::FastForwardFailed),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for HistoryEventFilterType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "HISTORY_EVENT_FILTER_TYPE_UNSPECIFIED",
            Self::AllEvent => "HISTORY_EVENT_FILTER_TYPE_ALL_EVENT",
            Self::CloseEvent => "HISTORY_EVENT_FILTER_TYPE_CLOSE_EVENT",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for HistoryEventFilterType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "HISTORY_EVENT_FILTER_TYPE_UNSPECIFIED",
            "HISTORY_EVENT_FILTER_TYPE_ALL_EVENT",
            "HISTORY_EVENT_FILTER_TYPE_CLOSE_EVENT",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = HistoryEventFilterType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "HISTORY_EVENT_FILTER_TYPE_UNSPECIFIED" => Ok(HistoryEventFilterType::Unspecified),
                    "HISTORY_EVENT_FILTER_TYPE_ALL_EVENT" => Ok(HistoryEventFilterType::AllEvent),
                    "HISTORY_EVENT_FILTER_TYPE_CLOSE_EVENT" => Ok(HistoryEventFilterType::CloseEvent),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for IndexedValueType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "INDEXED_VALUE_TYPE_UNSPECIFIED",
            Self::Text => "INDEXED_VALUE_TYPE_TEXT",
            Self::Keyword => "INDEXED_VALUE_TYPE_KEYWORD",
            Self::Int => "INDEXED_VALUE_TYPE_INT",
            Self::Double => "INDEXED_VALUE_TYPE_DOUBLE",
            Self::Bool => "INDEXED_VALUE_TYPE_BOOL",
            Self::Datetime => "INDEXED_VALUE_TYPE_DATETIME",
            Self::KeywordList => "INDEXED_VALUE_TYPE_KEYWORD_LIST",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for IndexedValueType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "INDEXED_VALUE_TYPE_UNSPECIFIED",
            "INDEXED_VALUE_TYPE_TEXT",
            "INDEXED_VALUE_TYPE_KEYWORD",
            "INDEXED_VALUE_TYPE_INT",
            "INDEXED_VALUE_TYPE_DOUBLE",
            "INDEXED_VALUE_TYPE_BOOL",
            "INDEXED_VALUE_TYPE_DATETIME",
            "INDEXED_VALUE_TYPE_KEYWORD_LIST",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = IndexedValueType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "INDEXED_VALUE_TYPE_UNSPECIFIED" => Ok(IndexedValueType::Unspecified),
                    "INDEXED_VALUE_TYPE_TEXT" => Ok(IndexedValueType::Text),
                    "INDEXED_VALUE_TYPE_KEYWORD" => Ok(IndexedValueType::Keyword),
                    "INDEXED_VALUE_TYPE_INT" => Ok(IndexedValueType::Int),
                    "INDEXED_VALUE_TYPE_DOUBLE" => Ok(IndexedValueType::Double),
                    "INDEXED_VALUE_TYPE_BOOL" => Ok(IndexedValueType::Bool),
                    "INDEXED_VALUE_TYPE_DATETIME" => Ok(IndexedValueType::Datetime),
                    "INDEXED_VALUE_TYPE_KEYWORD_LIST" => Ok(IndexedValueType::KeywordList),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for NamespaceState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "NAMESPACE_STATE_UNSPECIFIED",
            Self::Registered => "NAMESPACE_STATE_REGISTERED",
            Self::Deprecated => "NAMESPACE_STATE_DEPRECATED",
            Self::Deleted => "NAMESPACE_STATE_DELETED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for NamespaceState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "NAMESPACE_STATE_UNSPECIFIED",
            "NAMESPACE_STATE_REGISTERED",
            "NAMESPACE_STATE_DEPRECATED",
            "NAMESPACE_STATE_DELETED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = NamespaceState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "NAMESPACE_STATE_UNSPECIFIED" => Ok(NamespaceState::Unspecified),
                    "NAMESPACE_STATE_REGISTERED" => Ok(NamespaceState::Registered),
                    "NAMESPACE_STATE_DEPRECATED" => Ok(NamespaceState::Deprecated),
                    "NAMESPACE_STATE_DELETED" => Ok(NamespaceState::Deleted),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for NexusHandlerErrorRetryBehavior {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "NEXUS_HANDLER_ERROR_RETRY_BEHAVIOR_UNSPECIFIED",
            Self::Retryable => "NEXUS_HANDLER_ERROR_RETRY_BEHAVIOR_RETRYABLE",
            Self::NonRetryable => "NEXUS_HANDLER_ERROR_RETRY_BEHAVIOR_NON_RETRYABLE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for NexusHandlerErrorRetryBehavior {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "NEXUS_HANDLER_ERROR_RETRY_BEHAVIOR_UNSPECIFIED",
            "NEXUS_HANDLER_ERROR_RETRY_BEHAVIOR_RETRYABLE",
            "NEXUS_HANDLER_ERROR_RETRY_BEHAVIOR_NON_RETRYABLE",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = NexusHandlerErrorRetryBehavior;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "NEXUS_HANDLER_ERROR_RETRY_BEHAVIOR_UNSPECIFIED" => Ok(NexusHandlerErrorRetryBehavior::Unspecified),
                    "NEXUS_HANDLER_ERROR_RETRY_BEHAVIOR_RETRYABLE" => Ok(NexusHandlerErrorRetryBehavior::Retryable),
                    "NEXUS_HANDLER_ERROR_RETRY_BEHAVIOR_NON_RETRYABLE" => Ok(NexusHandlerErrorRetryBehavior::NonRetryable),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for NexusOperationCancellationState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "NEXUS_OPERATION_CANCELLATION_STATE_UNSPECIFIED",
            Self::Scheduled => "NEXUS_OPERATION_CANCELLATION_STATE_SCHEDULED",
            Self::BackingOff => "NEXUS_OPERATION_CANCELLATION_STATE_BACKING_OFF",
            Self::Succeeded => "NEXUS_OPERATION_CANCELLATION_STATE_SUCCEEDED",
            Self::Failed => "NEXUS_OPERATION_CANCELLATION_STATE_FAILED",
            Self::TimedOut => "NEXUS_OPERATION_CANCELLATION_STATE_TIMED_OUT",
            Self::Blocked => "NEXUS_OPERATION_CANCELLATION_STATE_BLOCKED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for NexusOperationCancellationState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "NEXUS_OPERATION_CANCELLATION_STATE_UNSPECIFIED",
            "NEXUS_OPERATION_CANCELLATION_STATE_SCHEDULED",
            "NEXUS_OPERATION_CANCELLATION_STATE_BACKING_OFF",
            "NEXUS_OPERATION_CANCELLATION_STATE_SUCCEEDED",
            "NEXUS_OPERATION_CANCELLATION_STATE_FAILED",
            "NEXUS_OPERATION_CANCELLATION_STATE_TIMED_OUT",
            "NEXUS_OPERATION_CANCELLATION_STATE_BLOCKED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = NexusOperationCancellationState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "NEXUS_OPERATION_CANCELLATION_STATE_UNSPECIFIED" => Ok(NexusOperationCancellationState::Unspecified),
                    "NEXUS_OPERATION_CANCELLATION_STATE_SCHEDULED" => Ok(NexusOperationCancellationState::Scheduled),
                    "NEXUS_OPERATION_CANCELLATION_STATE_BACKING_OFF" => Ok(NexusOperationCancellationState::BackingOff),
                    "NEXUS_OPERATION_CANCELLATION_STATE_SUCCEEDED" => Ok(NexusOperationCancellationState::Succeeded),
                    "NEXUS_OPERATION_CANCELLATION_STATE_FAILED" => Ok(NexusOperationCancellationState::Failed),
                    "NEXUS_OPERATION_CANCELLATION_STATE_TIMED_OUT" => Ok(NexusOperationCancellationState::TimedOut),
                    "NEXUS_OPERATION_CANCELLATION_STATE_BLOCKED" => Ok(NexusOperationCancellationState::Blocked),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for NexusOperationExecutionStatus {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "NEXUS_OPERATION_EXECUTION_STATUS_UNSPECIFIED",
            Self::Running => "NEXUS_OPERATION_EXECUTION_STATUS_RUNNING",
            Self::Completed => "NEXUS_OPERATION_EXECUTION_STATUS_COMPLETED",
            Self::Failed => "NEXUS_OPERATION_EXECUTION_STATUS_FAILED",
            Self::Canceled => "NEXUS_OPERATION_EXECUTION_STATUS_CANCELED",
            Self::Terminated => "NEXUS_OPERATION_EXECUTION_STATUS_TERMINATED",
            Self::TimedOut => "NEXUS_OPERATION_EXECUTION_STATUS_TIMED_OUT",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for NexusOperationExecutionStatus {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "NEXUS_OPERATION_EXECUTION_STATUS_UNSPECIFIED",
            "NEXUS_OPERATION_EXECUTION_STATUS_RUNNING",
            "NEXUS_OPERATION_EXECUTION_STATUS_COMPLETED",
            "NEXUS_OPERATION_EXECUTION_STATUS_FAILED",
            "NEXUS_OPERATION_EXECUTION_STATUS_CANCELED",
            "NEXUS_OPERATION_EXECUTION_STATUS_TERMINATED",
            "NEXUS_OPERATION_EXECUTION_STATUS_TIMED_OUT",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = NexusOperationExecutionStatus;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "NEXUS_OPERATION_EXECUTION_STATUS_UNSPECIFIED" => Ok(NexusOperationExecutionStatus::Unspecified),
                    "NEXUS_OPERATION_EXECUTION_STATUS_RUNNING" => Ok(NexusOperationExecutionStatus::Running),
                    "NEXUS_OPERATION_EXECUTION_STATUS_COMPLETED" => Ok(NexusOperationExecutionStatus::Completed),
                    "NEXUS_OPERATION_EXECUTION_STATUS_FAILED" => Ok(NexusOperationExecutionStatus::Failed),
                    "NEXUS_OPERATION_EXECUTION_STATUS_CANCELED" => Ok(NexusOperationExecutionStatus::Canceled),
                    "NEXUS_OPERATION_EXECUTION_STATUS_TERMINATED" => Ok(NexusOperationExecutionStatus::Terminated),
                    "NEXUS_OPERATION_EXECUTION_STATUS_TIMED_OUT" => Ok(NexusOperationExecutionStatus::TimedOut),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for NexusOperationIdConflictPolicy {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "NEXUS_OPERATION_ID_CONFLICT_POLICY_UNSPECIFIED",
            Self::Fail => "NEXUS_OPERATION_ID_CONFLICT_POLICY_FAIL",
            Self::UseExisting => "NEXUS_OPERATION_ID_CONFLICT_POLICY_USE_EXISTING",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for NexusOperationIdConflictPolicy {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "NEXUS_OPERATION_ID_CONFLICT_POLICY_UNSPECIFIED",
            "NEXUS_OPERATION_ID_CONFLICT_POLICY_FAIL",
            "NEXUS_OPERATION_ID_CONFLICT_POLICY_USE_EXISTING",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = NexusOperationIdConflictPolicy;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "NEXUS_OPERATION_ID_CONFLICT_POLICY_UNSPECIFIED" => Ok(NexusOperationIdConflictPolicy::Unspecified),
                    "NEXUS_OPERATION_ID_CONFLICT_POLICY_FAIL" => Ok(NexusOperationIdConflictPolicy::Fail),
                    "NEXUS_OPERATION_ID_CONFLICT_POLICY_USE_EXISTING" => Ok(NexusOperationIdConflictPolicy::UseExisting),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for NexusOperationIdReusePolicy {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "NEXUS_OPERATION_ID_REUSE_POLICY_UNSPECIFIED",
            Self::AllowDuplicate => "NEXUS_OPERATION_ID_REUSE_POLICY_ALLOW_DUPLICATE",
            Self::AllowDuplicateFailedOnly => "NEXUS_OPERATION_ID_REUSE_POLICY_ALLOW_DUPLICATE_FAILED_ONLY",
            Self::RejectDuplicate => "NEXUS_OPERATION_ID_REUSE_POLICY_REJECT_DUPLICATE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for NexusOperationIdReusePolicy {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "NEXUS_OPERATION_ID_REUSE_POLICY_UNSPECIFIED",
            "NEXUS_OPERATION_ID_REUSE_POLICY_ALLOW_DUPLICATE",
            "NEXUS_OPERATION_ID_REUSE_POLICY_ALLOW_DUPLICATE_FAILED_ONLY",
            "NEXUS_OPERATION_ID_REUSE_POLICY_REJECT_DUPLICATE",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = NexusOperationIdReusePolicy;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "NEXUS_OPERATION_ID_REUSE_POLICY_UNSPECIFIED" => Ok(NexusOperationIdReusePolicy::Unspecified),
                    "NEXUS_OPERATION_ID_REUSE_POLICY_ALLOW_DUPLICATE" => Ok(NexusOperationIdReusePolicy::AllowDuplicate),
                    "NEXUS_OPERATION_ID_REUSE_POLICY_ALLOW_DUPLICATE_FAILED_ONLY" => Ok(NexusOperationIdReusePolicy::AllowDuplicateFailedOnly),
                    "NEXUS_OPERATION_ID_REUSE_POLICY_REJECT_DUPLICATE" => Ok(NexusOperationIdReusePolicy::RejectDuplicate),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for NexusOperationWaitStage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "NEXUS_OPERATION_WAIT_STAGE_UNSPECIFIED",
            Self::Started => "NEXUS_OPERATION_WAIT_STAGE_STARTED",
            Self::Closed => "NEXUS_OPERATION_WAIT_STAGE_CLOSED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for NexusOperationWaitStage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "NEXUS_OPERATION_WAIT_STAGE_UNSPECIFIED",
            "NEXUS_OPERATION_WAIT_STAGE_STARTED",
            "NEXUS_OPERATION_WAIT_STAGE_CLOSED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = NexusOperationWaitStage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "NEXUS_OPERATION_WAIT_STAGE_UNSPECIFIED" => Ok(NexusOperationWaitStage::Unspecified),
                    "NEXUS_OPERATION_WAIT_STAGE_STARTED" => Ok(NexusOperationWaitStage::Started),
                    "NEXUS_OPERATION_WAIT_STAGE_CLOSED" => Ok(NexusOperationWaitStage::Closed),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ParentClosePolicy {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "PARENT_CLOSE_POLICY_UNSPECIFIED",
            Self::Terminate => "PARENT_CLOSE_POLICY_TERMINATE",
            Self::Abandon => "PARENT_CLOSE_POLICY_ABANDON",
            Self::RequestCancel => "PARENT_CLOSE_POLICY_REQUEST_CANCEL",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ParentClosePolicy {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "PARENT_CLOSE_POLICY_UNSPECIFIED",
            "PARENT_CLOSE_POLICY_TERMINATE",
            "PARENT_CLOSE_POLICY_ABANDON",
            "PARENT_CLOSE_POLICY_REQUEST_CANCEL",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ParentClosePolicy;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "PARENT_CLOSE_POLICY_UNSPECIFIED" => Ok(ParentClosePolicy::Unspecified),
                    "PARENT_CLOSE_POLICY_TERMINATE" => Ok(ParentClosePolicy::Terminate),
                    "PARENT_CLOSE_POLICY_ABANDON" => Ok(ParentClosePolicy::Abandon),
                    "PARENT_CLOSE_POLICY_REQUEST_CANCEL" => Ok(ParentClosePolicy::RequestCancel),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for PendingActivityState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "PENDING_ACTIVITY_STATE_UNSPECIFIED",
            Self::Scheduled => "PENDING_ACTIVITY_STATE_SCHEDULED",
            Self::Started => "PENDING_ACTIVITY_STATE_STARTED",
            Self::CancelRequested => "PENDING_ACTIVITY_STATE_CANCEL_REQUESTED",
            Self::Paused => "PENDING_ACTIVITY_STATE_PAUSED",
            Self::PauseRequested => "PENDING_ACTIVITY_STATE_PAUSE_REQUESTED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for PendingActivityState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "PENDING_ACTIVITY_STATE_UNSPECIFIED",
            "PENDING_ACTIVITY_STATE_SCHEDULED",
            "PENDING_ACTIVITY_STATE_STARTED",
            "PENDING_ACTIVITY_STATE_CANCEL_REQUESTED",
            "PENDING_ACTIVITY_STATE_PAUSED",
            "PENDING_ACTIVITY_STATE_PAUSE_REQUESTED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = PendingActivityState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "PENDING_ACTIVITY_STATE_UNSPECIFIED" => Ok(PendingActivityState::Unspecified),
                    "PENDING_ACTIVITY_STATE_SCHEDULED" => Ok(PendingActivityState::Scheduled),
                    "PENDING_ACTIVITY_STATE_STARTED" => Ok(PendingActivityState::Started),
                    "PENDING_ACTIVITY_STATE_CANCEL_REQUESTED" => Ok(PendingActivityState::CancelRequested),
                    "PENDING_ACTIVITY_STATE_PAUSED" => Ok(PendingActivityState::Paused),
                    "PENDING_ACTIVITY_STATE_PAUSE_REQUESTED" => Ok(PendingActivityState::PauseRequested),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for PendingNexusOperationState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "PENDING_NEXUS_OPERATION_STATE_UNSPECIFIED",
            Self::Scheduled => "PENDING_NEXUS_OPERATION_STATE_SCHEDULED",
            Self::BackingOff => "PENDING_NEXUS_OPERATION_STATE_BACKING_OFF",
            Self::Started => "PENDING_NEXUS_OPERATION_STATE_STARTED",
            Self::Blocked => "PENDING_NEXUS_OPERATION_STATE_BLOCKED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for PendingNexusOperationState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "PENDING_NEXUS_OPERATION_STATE_UNSPECIFIED",
            "PENDING_NEXUS_OPERATION_STATE_SCHEDULED",
            "PENDING_NEXUS_OPERATION_STATE_BACKING_OFF",
            "PENDING_NEXUS_OPERATION_STATE_STARTED",
            "PENDING_NEXUS_OPERATION_STATE_BLOCKED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = PendingNexusOperationState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "PENDING_NEXUS_OPERATION_STATE_UNSPECIFIED" => Ok(PendingNexusOperationState::Unspecified),
                    "PENDING_NEXUS_OPERATION_STATE_SCHEDULED" => Ok(PendingNexusOperationState::Scheduled),
                    "PENDING_NEXUS_OPERATION_STATE_BACKING_OFF" => Ok(PendingNexusOperationState::BackingOff),
                    "PENDING_NEXUS_OPERATION_STATE_STARTED" => Ok(PendingNexusOperationState::Started),
                    "PENDING_NEXUS_OPERATION_STATE_BLOCKED" => Ok(PendingNexusOperationState::Blocked),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for PendingWorkflowTaskState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "PENDING_WORKFLOW_TASK_STATE_UNSPECIFIED",
            Self::Scheduled => "PENDING_WORKFLOW_TASK_STATE_SCHEDULED",
            Self::Started => "PENDING_WORKFLOW_TASK_STATE_STARTED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for PendingWorkflowTaskState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "PENDING_WORKFLOW_TASK_STATE_UNSPECIFIED",
            "PENDING_WORKFLOW_TASK_STATE_SCHEDULED",
            "PENDING_WORKFLOW_TASK_STATE_STARTED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = PendingWorkflowTaskState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "PENDING_WORKFLOW_TASK_STATE_UNSPECIFIED" => Ok(PendingWorkflowTaskState::Unspecified),
                    "PENDING_WORKFLOW_TASK_STATE_SCHEDULED" => Ok(PendingWorkflowTaskState::Scheduled),
                    "PENDING_WORKFLOW_TASK_STATE_STARTED" => Ok(PendingWorkflowTaskState::Started),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for QueryRejectCondition {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "QUERY_REJECT_CONDITION_UNSPECIFIED",
            Self::None => "QUERY_REJECT_CONDITION_NONE",
            Self::NotOpen => "QUERY_REJECT_CONDITION_NOT_OPEN",
            Self::NotCompletedCleanly => "QUERY_REJECT_CONDITION_NOT_COMPLETED_CLEANLY",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for QueryRejectCondition {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "QUERY_REJECT_CONDITION_UNSPECIFIED",
            "QUERY_REJECT_CONDITION_NONE",
            "QUERY_REJECT_CONDITION_NOT_OPEN",
            "QUERY_REJECT_CONDITION_NOT_COMPLETED_CLEANLY",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = QueryRejectCondition;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "QUERY_REJECT_CONDITION_UNSPECIFIED" => Ok(QueryRejectCondition::Unspecified),
                    "QUERY_REJECT_CONDITION_NONE" => Ok(QueryRejectCondition::None),
                    "QUERY_REJECT_CONDITION_NOT_OPEN" => Ok(QueryRejectCondition::NotOpen),
                    "QUERY_REJECT_CONDITION_NOT_COMPLETED_CLEANLY" => Ok(QueryRejectCondition::NotCompletedCleanly),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for QueryResultType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "QUERY_RESULT_TYPE_UNSPECIFIED",
            Self::Answered => "QUERY_RESULT_TYPE_ANSWERED",
            Self::Failed => "QUERY_RESULT_TYPE_FAILED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for QueryResultType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "QUERY_RESULT_TYPE_UNSPECIFIED",
            "QUERY_RESULT_TYPE_ANSWERED",
            "QUERY_RESULT_TYPE_FAILED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = QueryResultType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "QUERY_RESULT_TYPE_UNSPECIFIED" => Ok(QueryResultType::Unspecified),
                    "QUERY_RESULT_TYPE_ANSWERED" => Ok(QueryResultType::Answered),
                    "QUERY_RESULT_TYPE_FAILED" => Ok(QueryResultType::Failed),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for RateLimitSource {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "RATE_LIMIT_SOURCE_UNSPECIFIED",
            Self::Api => "RATE_LIMIT_SOURCE_API",
            Self::Worker => "RATE_LIMIT_SOURCE_WORKER",
            Self::System => "RATE_LIMIT_SOURCE_SYSTEM",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for RateLimitSource {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "RATE_LIMIT_SOURCE_UNSPECIFIED",
            "RATE_LIMIT_SOURCE_API",
            "RATE_LIMIT_SOURCE_WORKER",
            "RATE_LIMIT_SOURCE_SYSTEM",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = RateLimitSource;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "RATE_LIMIT_SOURCE_UNSPECIFIED" => Ok(RateLimitSource::Unspecified),
                    "RATE_LIMIT_SOURCE_API" => Ok(RateLimitSource::Api),
                    "RATE_LIMIT_SOURCE_WORKER" => Ok(RateLimitSource::Worker),
                    "RATE_LIMIT_SOURCE_SYSTEM" => Ok(RateLimitSource::System),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ReplicationState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "REPLICATION_STATE_UNSPECIFIED",
            Self::Normal => "REPLICATION_STATE_NORMAL",
            Self::Handover => "REPLICATION_STATE_HANDOVER",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ReplicationState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "REPLICATION_STATE_UNSPECIFIED",
            "REPLICATION_STATE_NORMAL",
            "REPLICATION_STATE_HANDOVER",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ReplicationState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "REPLICATION_STATE_UNSPECIFIED" => Ok(ReplicationState::Unspecified),
                    "REPLICATION_STATE_NORMAL" => Ok(ReplicationState::Normal),
                    "REPLICATION_STATE_HANDOVER" => Ok(ReplicationState::Handover),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ResetReapplyExcludeType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "RESET_REAPPLY_EXCLUDE_TYPE_UNSPECIFIED",
            Self::Signal => "RESET_REAPPLY_EXCLUDE_TYPE_SIGNAL",
            Self::Update => "RESET_REAPPLY_EXCLUDE_TYPE_UPDATE",
            Self::Nexus => "RESET_REAPPLY_EXCLUDE_TYPE_NEXUS",
            Self::CancelRequest => "RESET_REAPPLY_EXCLUDE_TYPE_CANCEL_REQUEST",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ResetReapplyExcludeType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "RESET_REAPPLY_EXCLUDE_TYPE_UNSPECIFIED",
            "RESET_REAPPLY_EXCLUDE_TYPE_SIGNAL",
            "RESET_REAPPLY_EXCLUDE_TYPE_UPDATE",
            "RESET_REAPPLY_EXCLUDE_TYPE_NEXUS",
            "RESET_REAPPLY_EXCLUDE_TYPE_CANCEL_REQUEST",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ResetReapplyExcludeType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "RESET_REAPPLY_EXCLUDE_TYPE_UNSPECIFIED" => Ok(ResetReapplyExcludeType::Unspecified),
                    "RESET_REAPPLY_EXCLUDE_TYPE_SIGNAL" => Ok(ResetReapplyExcludeType::Signal),
                    "RESET_REAPPLY_EXCLUDE_TYPE_UPDATE" => Ok(ResetReapplyExcludeType::Update),
                    "RESET_REAPPLY_EXCLUDE_TYPE_NEXUS" => Ok(ResetReapplyExcludeType::Nexus),
                    "RESET_REAPPLY_EXCLUDE_TYPE_CANCEL_REQUEST" => Ok(ResetReapplyExcludeType::CancelRequest),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ResetReapplyType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "RESET_REAPPLY_TYPE_UNSPECIFIED",
            Self::Signal => "RESET_REAPPLY_TYPE_SIGNAL",
            Self::None => "RESET_REAPPLY_TYPE_NONE",
            Self::AllEligible => "RESET_REAPPLY_TYPE_ALL_ELIGIBLE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ResetReapplyType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "RESET_REAPPLY_TYPE_UNSPECIFIED",
            "RESET_REAPPLY_TYPE_SIGNAL",
            "RESET_REAPPLY_TYPE_NONE",
            "RESET_REAPPLY_TYPE_ALL_ELIGIBLE",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ResetReapplyType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "RESET_REAPPLY_TYPE_UNSPECIFIED" => Ok(ResetReapplyType::Unspecified),
                    "RESET_REAPPLY_TYPE_SIGNAL" => Ok(ResetReapplyType::Signal),
                    "RESET_REAPPLY_TYPE_NONE" => Ok(ResetReapplyType::None),
                    "RESET_REAPPLY_TYPE_ALL_ELIGIBLE" => Ok(ResetReapplyType::AllEligible),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ResetType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "RESET_TYPE_UNSPECIFIED",
            Self::FirstWorkflowTask => "RESET_TYPE_FIRST_WORKFLOW_TASK",
            Self::LastWorkflowTask => "RESET_TYPE_LAST_WORKFLOW_TASK",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ResetType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "RESET_TYPE_UNSPECIFIED",
            "RESET_TYPE_FIRST_WORKFLOW_TASK",
            "RESET_TYPE_LAST_WORKFLOW_TASK",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ResetType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "RESET_TYPE_UNSPECIFIED" => Ok(ResetType::Unspecified),
                    "RESET_TYPE_FIRST_WORKFLOW_TASK" => Ok(ResetType::FirstWorkflowTask),
                    "RESET_TYPE_LAST_WORKFLOW_TASK" => Ok(ResetType::LastWorkflowTask),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ResourceExhaustedCause {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "RESOURCE_EXHAUSTED_CAUSE_UNSPECIFIED",
            Self::RpsLimit => "RESOURCE_EXHAUSTED_CAUSE_RPS_LIMIT",
            Self::ConcurrentLimit => "RESOURCE_EXHAUSTED_CAUSE_CONCURRENT_LIMIT",
            Self::SystemOverloaded => "RESOURCE_EXHAUSTED_CAUSE_SYSTEM_OVERLOADED",
            Self::PersistenceLimit => "RESOURCE_EXHAUSTED_CAUSE_PERSISTENCE_LIMIT",
            Self::BusyWorkflow => "RESOURCE_EXHAUSTED_CAUSE_BUSY_WORKFLOW",
            Self::ApsLimit => "RESOURCE_EXHAUSTED_CAUSE_APS_LIMIT",
            Self::PersistenceStorageLimit => "RESOURCE_EXHAUSTED_CAUSE_PERSISTENCE_STORAGE_LIMIT",
            Self::CircuitBreakerOpen => "RESOURCE_EXHAUSTED_CAUSE_CIRCUIT_BREAKER_OPEN",
            Self::OpsLimit => "RESOURCE_EXHAUSTED_CAUSE_OPS_LIMIT",
            Self::WorkerDeploymentLimits => "RESOURCE_EXHAUSTED_CAUSE_WORKER_DEPLOYMENT_LIMITS",
            Self::BandwidthLimit => "RESOURCE_EXHAUSTED_CAUSE_BANDWIDTH_LIMIT",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ResourceExhaustedCause {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "RESOURCE_EXHAUSTED_CAUSE_UNSPECIFIED",
            "RESOURCE_EXHAUSTED_CAUSE_RPS_LIMIT",
            "RESOURCE_EXHAUSTED_CAUSE_CONCURRENT_LIMIT",
            "RESOURCE_EXHAUSTED_CAUSE_SYSTEM_OVERLOADED",
            "RESOURCE_EXHAUSTED_CAUSE_PERSISTENCE_LIMIT",
            "RESOURCE_EXHAUSTED_CAUSE_BUSY_WORKFLOW",
            "RESOURCE_EXHAUSTED_CAUSE_APS_LIMIT",
            "RESOURCE_EXHAUSTED_CAUSE_PERSISTENCE_STORAGE_LIMIT",
            "RESOURCE_EXHAUSTED_CAUSE_CIRCUIT_BREAKER_OPEN",
            "RESOURCE_EXHAUSTED_CAUSE_OPS_LIMIT",
            "RESOURCE_EXHAUSTED_CAUSE_WORKER_DEPLOYMENT_LIMITS",
            "RESOURCE_EXHAUSTED_CAUSE_BANDWIDTH_LIMIT",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ResourceExhaustedCause;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "RESOURCE_EXHAUSTED_CAUSE_UNSPECIFIED" => Ok(ResourceExhaustedCause::Unspecified),
                    "RESOURCE_EXHAUSTED_CAUSE_RPS_LIMIT" => Ok(ResourceExhaustedCause::RpsLimit),
                    "RESOURCE_EXHAUSTED_CAUSE_CONCURRENT_LIMIT" => Ok(ResourceExhaustedCause::ConcurrentLimit),
                    "RESOURCE_EXHAUSTED_CAUSE_SYSTEM_OVERLOADED" => Ok(ResourceExhaustedCause::SystemOverloaded),
                    "RESOURCE_EXHAUSTED_CAUSE_PERSISTENCE_LIMIT" => Ok(ResourceExhaustedCause::PersistenceLimit),
                    "RESOURCE_EXHAUSTED_CAUSE_BUSY_WORKFLOW" => Ok(ResourceExhaustedCause::BusyWorkflow),
                    "RESOURCE_EXHAUSTED_CAUSE_APS_LIMIT" => Ok(ResourceExhaustedCause::ApsLimit),
                    "RESOURCE_EXHAUSTED_CAUSE_PERSISTENCE_STORAGE_LIMIT" => Ok(ResourceExhaustedCause::PersistenceStorageLimit),
                    "RESOURCE_EXHAUSTED_CAUSE_CIRCUIT_BREAKER_OPEN" => Ok(ResourceExhaustedCause::CircuitBreakerOpen),
                    "RESOURCE_EXHAUSTED_CAUSE_OPS_LIMIT" => Ok(ResourceExhaustedCause::OpsLimit),
                    "RESOURCE_EXHAUSTED_CAUSE_WORKER_DEPLOYMENT_LIMITS" => Ok(ResourceExhaustedCause::WorkerDeploymentLimits),
                    "RESOURCE_EXHAUSTED_CAUSE_BANDWIDTH_LIMIT" => Ok(ResourceExhaustedCause::BandwidthLimit),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ResourceExhaustedScope {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "RESOURCE_EXHAUSTED_SCOPE_UNSPECIFIED",
            Self::Namespace => "RESOURCE_EXHAUSTED_SCOPE_NAMESPACE",
            Self::System => "RESOURCE_EXHAUSTED_SCOPE_SYSTEM",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ResourceExhaustedScope {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "RESOURCE_EXHAUSTED_SCOPE_UNSPECIFIED",
            "RESOURCE_EXHAUSTED_SCOPE_NAMESPACE",
            "RESOURCE_EXHAUSTED_SCOPE_SYSTEM",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ResourceExhaustedScope;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "RESOURCE_EXHAUSTED_SCOPE_UNSPECIFIED" => Ok(ResourceExhaustedScope::Unspecified),
                    "RESOURCE_EXHAUSTED_SCOPE_NAMESPACE" => Ok(ResourceExhaustedScope::Namespace),
                    "RESOURCE_EXHAUSTED_SCOPE_SYSTEM" => Ok(ResourceExhaustedScope::System),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for RetryState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "RETRY_STATE_UNSPECIFIED",
            Self::InProgress => "RETRY_STATE_IN_PROGRESS",
            Self::NonRetryableFailure => "RETRY_STATE_NON_RETRYABLE_FAILURE",
            Self::Timeout => "RETRY_STATE_TIMEOUT",
            Self::MaximumAttemptsReached => "RETRY_STATE_MAXIMUM_ATTEMPTS_REACHED",
            Self::RetryPolicyNotSet => "RETRY_STATE_RETRY_POLICY_NOT_SET",
            Self::InternalServerError => "RETRY_STATE_INTERNAL_SERVER_ERROR",
            Self::CancelRequested => "RETRY_STATE_CANCEL_REQUESTED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for RetryState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "RETRY_STATE_UNSPECIFIED",
            "RETRY_STATE_IN_PROGRESS",
            "RETRY_STATE_NON_RETRYABLE_FAILURE",
            "RETRY_STATE_TIMEOUT",
            "RETRY_STATE_MAXIMUM_ATTEMPTS_REACHED",
            "RETRY_STATE_RETRY_POLICY_NOT_SET",
            "RETRY_STATE_INTERNAL_SERVER_ERROR",
            "RETRY_STATE_CANCEL_REQUESTED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = RetryState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "RETRY_STATE_UNSPECIFIED" => Ok(RetryState::Unspecified),
                    "RETRY_STATE_IN_PROGRESS" => Ok(RetryState::InProgress),
                    "RETRY_STATE_NON_RETRYABLE_FAILURE" => Ok(RetryState::NonRetryableFailure),
                    "RETRY_STATE_TIMEOUT" => Ok(RetryState::Timeout),
                    "RETRY_STATE_MAXIMUM_ATTEMPTS_REACHED" => Ok(RetryState::MaximumAttemptsReached),
                    "RETRY_STATE_RETRY_POLICY_NOT_SET" => Ok(RetryState::RetryPolicyNotSet),
                    "RETRY_STATE_INTERNAL_SERVER_ERROR" => Ok(RetryState::InternalServerError),
                    "RETRY_STATE_CANCEL_REQUESTED" => Ok(RetryState::CancelRequested),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for RoutingConfigUpdateState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "ROUTING_CONFIG_UPDATE_STATE_UNSPECIFIED",
            Self::InProgress => "ROUTING_CONFIG_UPDATE_STATE_IN_PROGRESS",
            Self::Completed => "ROUTING_CONFIG_UPDATE_STATE_COMPLETED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for RoutingConfigUpdateState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ROUTING_CONFIG_UPDATE_STATE_UNSPECIFIED",
            "ROUTING_CONFIG_UPDATE_STATE_IN_PROGRESS",
            "ROUTING_CONFIG_UPDATE_STATE_COMPLETED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = RoutingConfigUpdateState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "ROUTING_CONFIG_UPDATE_STATE_UNSPECIFIED" => Ok(RoutingConfigUpdateState::Unspecified),
                    "ROUTING_CONFIG_UPDATE_STATE_IN_PROGRESS" => Ok(RoutingConfigUpdateState::InProgress),
                    "ROUTING_CONFIG_UPDATE_STATE_COMPLETED" => Ok(RoutingConfigUpdateState::Completed),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ScheduleOverlapPolicy {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "SCHEDULE_OVERLAP_POLICY_UNSPECIFIED",
            Self::Skip => "SCHEDULE_OVERLAP_POLICY_SKIP",
            Self::BufferOne => "SCHEDULE_OVERLAP_POLICY_BUFFER_ONE",
            Self::BufferAll => "SCHEDULE_OVERLAP_POLICY_BUFFER_ALL",
            Self::CancelOther => "SCHEDULE_OVERLAP_POLICY_CANCEL_OTHER",
            Self::TerminateOther => "SCHEDULE_OVERLAP_POLICY_TERMINATE_OTHER",
            Self::AllowAll => "SCHEDULE_OVERLAP_POLICY_ALLOW_ALL",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ScheduleOverlapPolicy {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "SCHEDULE_OVERLAP_POLICY_UNSPECIFIED",
            "SCHEDULE_OVERLAP_POLICY_SKIP",
            "SCHEDULE_OVERLAP_POLICY_BUFFER_ONE",
            "SCHEDULE_OVERLAP_POLICY_BUFFER_ALL",
            "SCHEDULE_OVERLAP_POLICY_CANCEL_OTHER",
            "SCHEDULE_OVERLAP_POLICY_TERMINATE_OTHER",
            "SCHEDULE_OVERLAP_POLICY_ALLOW_ALL",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = ScheduleOverlapPolicy;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "SCHEDULE_OVERLAP_POLICY_UNSPECIFIED" => Ok(ScheduleOverlapPolicy::Unspecified),
                    "SCHEDULE_OVERLAP_POLICY_SKIP" => Ok(ScheduleOverlapPolicy::Skip),
                    "SCHEDULE_OVERLAP_POLICY_BUFFER_ONE" => Ok(ScheduleOverlapPolicy::BufferOne),
                    "SCHEDULE_OVERLAP_POLICY_BUFFER_ALL" => Ok(ScheduleOverlapPolicy::BufferAll),
                    "SCHEDULE_OVERLAP_POLICY_CANCEL_OTHER" => Ok(ScheduleOverlapPolicy::CancelOther),
                    "SCHEDULE_OVERLAP_POLICY_TERMINATE_OTHER" => Ok(ScheduleOverlapPolicy::TerminateOther),
                    "SCHEDULE_OVERLAP_POLICY_ALLOW_ALL" => Ok(ScheduleOverlapPolicy::AllowAll),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for Severity {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "SEVERITY_UNSPECIFIED",
            Self::High => "SEVERITY_HIGH",
            Self::Medium => "SEVERITY_MEDIUM",
            Self::Low => "SEVERITY_LOW",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for Severity {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "SEVERITY_UNSPECIFIED",
            "SEVERITY_HIGH",
            "SEVERITY_MEDIUM",
            "SEVERITY_LOW",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = Severity;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "SEVERITY_UNSPECIFIED" => Ok(Severity::Unspecified),
                    "SEVERITY_HIGH" => Ok(Severity::High),
                    "SEVERITY_MEDIUM" => Ok(Severity::Medium),
                    "SEVERITY_LOW" => Ok(Severity::Low),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for SignalExternalWorkflowExecutionFailedCause {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_UNSPECIFIED",
            Self::ExternalWorkflowExecutionNotFound => "SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_EXTERNAL_WORKFLOW_EXECUTION_NOT_FOUND",
            Self::NamespaceNotFound => "SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_NAMESPACE_NOT_FOUND",
            Self::SignalCountLimitExceeded => "SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_SIGNAL_COUNT_LIMIT_EXCEEDED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for SignalExternalWorkflowExecutionFailedCause {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_UNSPECIFIED",
            "SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_EXTERNAL_WORKFLOW_EXECUTION_NOT_FOUND",
            "SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_NAMESPACE_NOT_FOUND",
            "SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_SIGNAL_COUNT_LIMIT_EXCEEDED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = SignalExternalWorkflowExecutionFailedCause;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_UNSPECIFIED" => Ok(SignalExternalWorkflowExecutionFailedCause::Unspecified),
                    "SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_EXTERNAL_WORKFLOW_EXECUTION_NOT_FOUND" => Ok(SignalExternalWorkflowExecutionFailedCause::ExternalWorkflowExecutionNotFound),
                    "SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_NAMESPACE_NOT_FOUND" => Ok(SignalExternalWorkflowExecutionFailedCause::NamespaceNotFound),
                    "SIGNAL_EXTERNAL_WORKFLOW_EXECUTION_FAILED_CAUSE_SIGNAL_COUNT_LIMIT_EXCEEDED" => Ok(SignalExternalWorkflowExecutionFailedCause::SignalCountLimitExceeded),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for StartChildWorkflowExecutionFailedCause {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "START_CHILD_WORKFLOW_EXECUTION_FAILED_CAUSE_UNSPECIFIED",
            Self::WorkflowAlreadyExists => "START_CHILD_WORKFLOW_EXECUTION_FAILED_CAUSE_WORKFLOW_ALREADY_EXISTS",
            Self::NamespaceNotFound => "START_CHILD_WORKFLOW_EXECUTION_FAILED_CAUSE_NAMESPACE_NOT_FOUND",
            Self::InvalidVersioningOverride => "START_CHILD_WORKFLOW_EXECUTION_FAILED_CAUSE_INVALID_VERSIONING_OVERRIDE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for StartChildWorkflowExecutionFailedCause {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "START_CHILD_WORKFLOW_EXECUTION_FAILED_CAUSE_UNSPECIFIED",
            "START_CHILD_WORKFLOW_EXECUTION_FAILED_CAUSE_WORKFLOW_ALREADY_EXISTS",
            "START_CHILD_WORKFLOW_EXECUTION_FAILED_CAUSE_NAMESPACE_NOT_FOUND",
            "START_CHILD_WORKFLOW_EXECUTION_FAILED_CAUSE_INVALID_VERSIONING_OVERRIDE",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = StartChildWorkflowExecutionFailedCause;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "START_CHILD_WORKFLOW_EXECUTION_FAILED_CAUSE_UNSPECIFIED" => Ok(StartChildWorkflowExecutionFailedCause::Unspecified),
                    "START_CHILD_WORKFLOW_EXECUTION_FAILED_CAUSE_WORKFLOW_ALREADY_EXISTS" => Ok(StartChildWorkflowExecutionFailedCause::WorkflowAlreadyExists),
                    "START_CHILD_WORKFLOW_EXECUTION_FAILED_CAUSE_NAMESPACE_NOT_FOUND" => Ok(StartChildWorkflowExecutionFailedCause::NamespaceNotFound),
                    "START_CHILD_WORKFLOW_EXECUTION_FAILED_CAUSE_INVALID_VERSIONING_OVERRIDE" => Ok(StartChildWorkflowExecutionFailedCause::InvalidVersioningOverride),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for SuggestContinueAsNewReason {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "SUGGEST_CONTINUE_AS_NEW_REASON_UNSPECIFIED",
            Self::HistorySizeTooLarge => "SUGGEST_CONTINUE_AS_NEW_REASON_HISTORY_SIZE_TOO_LARGE",
            Self::TooManyHistoryEvents => "SUGGEST_CONTINUE_AS_NEW_REASON_TOO_MANY_HISTORY_EVENTS",
            Self::TooManyUpdates => "SUGGEST_CONTINUE_AS_NEW_REASON_TOO_MANY_UPDATES",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for SuggestContinueAsNewReason {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "SUGGEST_CONTINUE_AS_NEW_REASON_UNSPECIFIED",
            "SUGGEST_CONTINUE_AS_NEW_REASON_HISTORY_SIZE_TOO_LARGE",
            "SUGGEST_CONTINUE_AS_NEW_REASON_TOO_MANY_HISTORY_EVENTS",
            "SUGGEST_CONTINUE_AS_NEW_REASON_TOO_MANY_UPDATES",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = SuggestContinueAsNewReason;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "SUGGEST_CONTINUE_AS_NEW_REASON_UNSPECIFIED" => Ok(SuggestContinueAsNewReason::Unspecified),
                    "SUGGEST_CONTINUE_AS_NEW_REASON_HISTORY_SIZE_TOO_LARGE" => Ok(SuggestContinueAsNewReason::HistorySizeTooLarge),
                    "SUGGEST_CONTINUE_AS_NEW_REASON_TOO_MANY_HISTORY_EVENTS" => Ok(SuggestContinueAsNewReason::TooManyHistoryEvents),
                    "SUGGEST_CONTINUE_AS_NEW_REASON_TOO_MANY_UPDATES" => Ok(SuggestContinueAsNewReason::TooManyUpdates),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for TaskQueueKind {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "TASK_QUEUE_KIND_UNSPECIFIED",
            Self::Normal => "TASK_QUEUE_KIND_NORMAL",
            Self::Sticky => "TASK_QUEUE_KIND_STICKY",
            Self::WorkerCommands => "TASK_QUEUE_KIND_WORKER_COMMANDS",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for TaskQueueKind {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "TASK_QUEUE_KIND_UNSPECIFIED",
            "TASK_QUEUE_KIND_NORMAL",
            "TASK_QUEUE_KIND_STICKY",
            "TASK_QUEUE_KIND_WORKER_COMMANDS",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = TaskQueueKind;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "TASK_QUEUE_KIND_UNSPECIFIED" => Ok(TaskQueueKind::Unspecified),
                    "TASK_QUEUE_KIND_NORMAL" => Ok(TaskQueueKind::Normal),
                    "TASK_QUEUE_KIND_STICKY" => Ok(TaskQueueKind::Sticky),
                    "TASK_QUEUE_KIND_WORKER_COMMANDS" => Ok(TaskQueueKind::WorkerCommands),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for TaskQueueType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "TASK_QUEUE_TYPE_UNSPECIFIED",
            Self::Workflow => "TASK_QUEUE_TYPE_WORKFLOW",
            Self::Activity => "TASK_QUEUE_TYPE_ACTIVITY",
            Self::Nexus => "TASK_QUEUE_TYPE_NEXUS",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for TaskQueueType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "TASK_QUEUE_TYPE_UNSPECIFIED",
            "TASK_QUEUE_TYPE_WORKFLOW",
            "TASK_QUEUE_TYPE_ACTIVITY",
            "TASK_QUEUE_TYPE_NEXUS",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = TaskQueueType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "TASK_QUEUE_TYPE_UNSPECIFIED" => Ok(TaskQueueType::Unspecified),
                    "TASK_QUEUE_TYPE_WORKFLOW" => Ok(TaskQueueType::Workflow),
                    "TASK_QUEUE_TYPE_ACTIVITY" => Ok(TaskQueueType::Activity),
                    "TASK_QUEUE_TYPE_NEXUS" => Ok(TaskQueueType::Nexus),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for TaskReachability {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "TASK_REACHABILITY_UNSPECIFIED",
            Self::NewWorkflows => "TASK_REACHABILITY_NEW_WORKFLOWS",
            Self::ExistingWorkflows => "TASK_REACHABILITY_EXISTING_WORKFLOWS",
            Self::OpenWorkflows => "TASK_REACHABILITY_OPEN_WORKFLOWS",
            Self::ClosedWorkflows => "TASK_REACHABILITY_CLOSED_WORKFLOWS",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for TaskReachability {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "TASK_REACHABILITY_UNSPECIFIED",
            "TASK_REACHABILITY_NEW_WORKFLOWS",
            "TASK_REACHABILITY_EXISTING_WORKFLOWS",
            "TASK_REACHABILITY_OPEN_WORKFLOWS",
            "TASK_REACHABILITY_CLOSED_WORKFLOWS",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = TaskReachability;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "TASK_REACHABILITY_UNSPECIFIED" => Ok(TaskReachability::Unspecified),
                    "TASK_REACHABILITY_NEW_WORKFLOWS" => Ok(TaskReachability::NewWorkflows),
                    "TASK_REACHABILITY_EXISTING_WORKFLOWS" => Ok(TaskReachability::ExistingWorkflows),
                    "TASK_REACHABILITY_OPEN_WORKFLOWS" => Ok(TaskReachability::OpenWorkflows),
                    "TASK_REACHABILITY_CLOSED_WORKFLOWS" => Ok(TaskReachability::ClosedWorkflows),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for TimeoutType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "TIMEOUT_TYPE_UNSPECIFIED",
            Self::StartToClose => "TIMEOUT_TYPE_START_TO_CLOSE",
            Self::ScheduleToStart => "TIMEOUT_TYPE_SCHEDULE_TO_START",
            Self::ScheduleToClose => "TIMEOUT_TYPE_SCHEDULE_TO_CLOSE",
            Self::Heartbeat => "TIMEOUT_TYPE_HEARTBEAT",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for TimeoutType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "TIMEOUT_TYPE_UNSPECIFIED",
            "TIMEOUT_TYPE_START_TO_CLOSE",
            "TIMEOUT_TYPE_SCHEDULE_TO_START",
            "TIMEOUT_TYPE_SCHEDULE_TO_CLOSE",
            "TIMEOUT_TYPE_HEARTBEAT",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = TimeoutType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "TIMEOUT_TYPE_UNSPECIFIED" => Ok(TimeoutType::Unspecified),
                    "TIMEOUT_TYPE_START_TO_CLOSE" => Ok(TimeoutType::StartToClose),
                    "TIMEOUT_TYPE_SCHEDULE_TO_START" => Ok(TimeoutType::ScheduleToStart),
                    "TIMEOUT_TYPE_SCHEDULE_TO_CLOSE" => Ok(TimeoutType::ScheduleToClose),
                    "TIMEOUT_TYPE_HEARTBEAT" => Ok(TimeoutType::Heartbeat),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateAdmittedEventOrigin {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "UPDATE_ADMITTED_EVENT_ORIGIN_UNSPECIFIED",
            Self::Reapply => "UPDATE_ADMITTED_EVENT_ORIGIN_REAPPLY",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for UpdateAdmittedEventOrigin {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "UPDATE_ADMITTED_EVENT_ORIGIN_UNSPECIFIED",
            "UPDATE_ADMITTED_EVENT_ORIGIN_REAPPLY",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = UpdateAdmittedEventOrigin;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "UPDATE_ADMITTED_EVENT_ORIGIN_UNSPECIFIED" => Ok(UpdateAdmittedEventOrigin::Unspecified),
                    "UPDATE_ADMITTED_EVENT_ORIGIN_REAPPLY" => Ok(UpdateAdmittedEventOrigin::Reapply),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateWorkflowExecutionLifecycleStage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "UPDATE_WORKFLOW_EXECUTION_LIFECYCLE_STAGE_UNSPECIFIED",
            Self::Admitted => "UPDATE_WORKFLOW_EXECUTION_LIFECYCLE_STAGE_ADMITTED",
            Self::Accepted => "UPDATE_WORKFLOW_EXECUTION_LIFECYCLE_STAGE_ACCEPTED",
            Self::Completed => "UPDATE_WORKFLOW_EXECUTION_LIFECYCLE_STAGE_COMPLETED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for UpdateWorkflowExecutionLifecycleStage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "UPDATE_WORKFLOW_EXECUTION_LIFECYCLE_STAGE_UNSPECIFIED",
            "UPDATE_WORKFLOW_EXECUTION_LIFECYCLE_STAGE_ADMITTED",
            "UPDATE_WORKFLOW_EXECUTION_LIFECYCLE_STAGE_ACCEPTED",
            "UPDATE_WORKFLOW_EXECUTION_LIFECYCLE_STAGE_COMPLETED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = UpdateWorkflowExecutionLifecycleStage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "UPDATE_WORKFLOW_EXECUTION_LIFECYCLE_STAGE_UNSPECIFIED" => Ok(UpdateWorkflowExecutionLifecycleStage::Unspecified),
                    "UPDATE_WORKFLOW_EXECUTION_LIFECYCLE_STAGE_ADMITTED" => Ok(UpdateWorkflowExecutionLifecycleStage::Admitted),
                    "UPDATE_WORKFLOW_EXECUTION_LIFECYCLE_STAGE_ACCEPTED" => Ok(UpdateWorkflowExecutionLifecycleStage::Accepted),
                    "UPDATE_WORKFLOW_EXECUTION_LIFECYCLE_STAGE_COMPLETED" => Ok(UpdateWorkflowExecutionLifecycleStage::Completed),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for VersionDrainageStatus {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "VERSION_DRAINAGE_STATUS_UNSPECIFIED",
            Self::Draining => "VERSION_DRAINAGE_STATUS_DRAINING",
            Self::Drained => "VERSION_DRAINAGE_STATUS_DRAINED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for VersionDrainageStatus {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "VERSION_DRAINAGE_STATUS_UNSPECIFIED",
            "VERSION_DRAINAGE_STATUS_DRAINING",
            "VERSION_DRAINAGE_STATUS_DRAINED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = VersionDrainageStatus;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "VERSION_DRAINAGE_STATUS_UNSPECIFIED" => Ok(VersionDrainageStatus::Unspecified),
                    "VERSION_DRAINAGE_STATUS_DRAINING" => Ok(VersionDrainageStatus::Draining),
                    "VERSION_DRAINAGE_STATUS_DRAINED" => Ok(VersionDrainageStatus::Drained),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for VersioningBehavior {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "VERSIONING_BEHAVIOR_UNSPECIFIED",
            Self::Pinned => "VERSIONING_BEHAVIOR_PINNED",
            Self::AutoUpgrade => "VERSIONING_BEHAVIOR_AUTO_UPGRADE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for VersioningBehavior {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "VERSIONING_BEHAVIOR_UNSPECIFIED",
            "VERSIONING_BEHAVIOR_PINNED",
            "VERSIONING_BEHAVIOR_AUTO_UPGRADE",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = VersioningBehavior;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "VERSIONING_BEHAVIOR_UNSPECIFIED" => Ok(VersioningBehavior::Unspecified),
                    "VERSIONING_BEHAVIOR_PINNED" => Ok(VersioningBehavior::Pinned),
                    "VERSIONING_BEHAVIOR_AUTO_UPGRADE" => Ok(VersioningBehavior::AutoUpgrade),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for WorkerDeploymentVersionStatus {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "WORKER_DEPLOYMENT_VERSION_STATUS_UNSPECIFIED",
            Self::Inactive => "WORKER_DEPLOYMENT_VERSION_STATUS_INACTIVE",
            Self::Current => "WORKER_DEPLOYMENT_VERSION_STATUS_CURRENT",
            Self::Ramping => "WORKER_DEPLOYMENT_VERSION_STATUS_RAMPING",
            Self::Draining => "WORKER_DEPLOYMENT_VERSION_STATUS_DRAINING",
            Self::Drained => "WORKER_DEPLOYMENT_VERSION_STATUS_DRAINED",
            Self::Created => "WORKER_DEPLOYMENT_VERSION_STATUS_CREATED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for WorkerDeploymentVersionStatus {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "WORKER_DEPLOYMENT_VERSION_STATUS_UNSPECIFIED",
            "WORKER_DEPLOYMENT_VERSION_STATUS_INACTIVE",
            "WORKER_DEPLOYMENT_VERSION_STATUS_CURRENT",
            "WORKER_DEPLOYMENT_VERSION_STATUS_RAMPING",
            "WORKER_DEPLOYMENT_VERSION_STATUS_DRAINING",
            "WORKER_DEPLOYMENT_VERSION_STATUS_DRAINED",
            "WORKER_DEPLOYMENT_VERSION_STATUS_CREATED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = WorkerDeploymentVersionStatus;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "WORKER_DEPLOYMENT_VERSION_STATUS_UNSPECIFIED" => Ok(WorkerDeploymentVersionStatus::Unspecified),
                    "WORKER_DEPLOYMENT_VERSION_STATUS_INACTIVE" => Ok(WorkerDeploymentVersionStatus::Inactive),
                    "WORKER_DEPLOYMENT_VERSION_STATUS_CURRENT" => Ok(WorkerDeploymentVersionStatus::Current),
                    "WORKER_DEPLOYMENT_VERSION_STATUS_RAMPING" => Ok(WorkerDeploymentVersionStatus::Ramping),
                    "WORKER_DEPLOYMENT_VERSION_STATUS_DRAINING" => Ok(WorkerDeploymentVersionStatus::Draining),
                    "WORKER_DEPLOYMENT_VERSION_STATUS_DRAINED" => Ok(WorkerDeploymentVersionStatus::Drained),
                    "WORKER_DEPLOYMENT_VERSION_STATUS_CREATED" => Ok(WorkerDeploymentVersionStatus::Created),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for WorkerStatus {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "WORKER_STATUS_UNSPECIFIED",
            Self::Running => "WORKER_STATUS_RUNNING",
            Self::ShuttingDown => "WORKER_STATUS_SHUTTING_DOWN",
            Self::Shutdown => "WORKER_STATUS_SHUTDOWN",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for WorkerStatus {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "WORKER_STATUS_UNSPECIFIED",
            "WORKER_STATUS_RUNNING",
            "WORKER_STATUS_SHUTTING_DOWN",
            "WORKER_STATUS_SHUTDOWN",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = WorkerStatus;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "WORKER_STATUS_UNSPECIFIED" => Ok(WorkerStatus::Unspecified),
                    "WORKER_STATUS_RUNNING" => Ok(WorkerStatus::Running),
                    "WORKER_STATUS_SHUTTING_DOWN" => Ok(WorkerStatus::ShuttingDown),
                    "WORKER_STATUS_SHUTDOWN" => Ok(WorkerStatus::Shutdown),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for WorkerVersioningMode {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "WORKER_VERSIONING_MODE_UNSPECIFIED",
            Self::Unversioned => "WORKER_VERSIONING_MODE_UNVERSIONED",
            Self::Versioned => "WORKER_VERSIONING_MODE_VERSIONED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for WorkerVersioningMode {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "WORKER_VERSIONING_MODE_UNSPECIFIED",
            "WORKER_VERSIONING_MODE_UNVERSIONED",
            "WORKER_VERSIONING_MODE_VERSIONED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = WorkerVersioningMode;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "WORKER_VERSIONING_MODE_UNSPECIFIED" => Ok(WorkerVersioningMode::Unspecified),
                    "WORKER_VERSIONING_MODE_UNVERSIONED" => Ok(WorkerVersioningMode::Unversioned),
                    "WORKER_VERSIONING_MODE_VERSIONED" => Ok(WorkerVersioningMode::Versioned),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for WorkflowExecutionStatus {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "WORKFLOW_EXECUTION_STATUS_UNSPECIFIED",
            Self::Running => "WORKFLOW_EXECUTION_STATUS_RUNNING",
            Self::Completed => "WORKFLOW_EXECUTION_STATUS_COMPLETED",
            Self::Failed => "WORKFLOW_EXECUTION_STATUS_FAILED",
            Self::Canceled => "WORKFLOW_EXECUTION_STATUS_CANCELED",
            Self::Terminated => "WORKFLOW_EXECUTION_STATUS_TERMINATED",
            Self::ContinuedAsNew => "WORKFLOW_EXECUTION_STATUS_CONTINUED_AS_NEW",
            Self::TimedOut => "WORKFLOW_EXECUTION_STATUS_TIMED_OUT",
            Self::Paused => "WORKFLOW_EXECUTION_STATUS_PAUSED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for WorkflowExecutionStatus {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "WORKFLOW_EXECUTION_STATUS_UNSPECIFIED",
            "WORKFLOW_EXECUTION_STATUS_RUNNING",
            "WORKFLOW_EXECUTION_STATUS_COMPLETED",
            "WORKFLOW_EXECUTION_STATUS_FAILED",
            "WORKFLOW_EXECUTION_STATUS_CANCELED",
            "WORKFLOW_EXECUTION_STATUS_TERMINATED",
            "WORKFLOW_EXECUTION_STATUS_CONTINUED_AS_NEW",
            "WORKFLOW_EXECUTION_STATUS_TIMED_OUT",
            "WORKFLOW_EXECUTION_STATUS_PAUSED",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = WorkflowExecutionStatus;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "WORKFLOW_EXECUTION_STATUS_UNSPECIFIED" => Ok(WorkflowExecutionStatus::Unspecified),
                    "WORKFLOW_EXECUTION_STATUS_RUNNING" => Ok(WorkflowExecutionStatus::Running),
                    "WORKFLOW_EXECUTION_STATUS_COMPLETED" => Ok(WorkflowExecutionStatus::Completed),
                    "WORKFLOW_EXECUTION_STATUS_FAILED" => Ok(WorkflowExecutionStatus::Failed),
                    "WORKFLOW_EXECUTION_STATUS_CANCELED" => Ok(WorkflowExecutionStatus::Canceled),
                    "WORKFLOW_EXECUTION_STATUS_TERMINATED" => Ok(WorkflowExecutionStatus::Terminated),
                    "WORKFLOW_EXECUTION_STATUS_CONTINUED_AS_NEW" => Ok(WorkflowExecutionStatus::ContinuedAsNew),
                    "WORKFLOW_EXECUTION_STATUS_TIMED_OUT" => Ok(WorkflowExecutionStatus::TimedOut),
                    "WORKFLOW_EXECUTION_STATUS_PAUSED" => Ok(WorkflowExecutionStatus::Paused),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for WorkflowIdConflictPolicy {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "WORKFLOW_ID_CONFLICT_POLICY_UNSPECIFIED",
            Self::Fail => "WORKFLOW_ID_CONFLICT_POLICY_FAIL",
            Self::UseExisting => "WORKFLOW_ID_CONFLICT_POLICY_USE_EXISTING",
            Self::TerminateExisting => "WORKFLOW_ID_CONFLICT_POLICY_TERMINATE_EXISTING",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for WorkflowIdConflictPolicy {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "WORKFLOW_ID_CONFLICT_POLICY_UNSPECIFIED",
            "WORKFLOW_ID_CONFLICT_POLICY_FAIL",
            "WORKFLOW_ID_CONFLICT_POLICY_USE_EXISTING",
            "WORKFLOW_ID_CONFLICT_POLICY_TERMINATE_EXISTING",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = WorkflowIdConflictPolicy;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "WORKFLOW_ID_CONFLICT_POLICY_UNSPECIFIED" => Ok(WorkflowIdConflictPolicy::Unspecified),
                    "WORKFLOW_ID_CONFLICT_POLICY_FAIL" => Ok(WorkflowIdConflictPolicy::Fail),
                    "WORKFLOW_ID_CONFLICT_POLICY_USE_EXISTING" => Ok(WorkflowIdConflictPolicy::UseExisting),
                    "WORKFLOW_ID_CONFLICT_POLICY_TERMINATE_EXISTING" => Ok(WorkflowIdConflictPolicy::TerminateExisting),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for WorkflowIdReusePolicy {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "WORKFLOW_ID_REUSE_POLICY_UNSPECIFIED",
            Self::AllowDuplicate => "WORKFLOW_ID_REUSE_POLICY_ALLOW_DUPLICATE",
            Self::AllowDuplicateFailedOnly => "WORKFLOW_ID_REUSE_POLICY_ALLOW_DUPLICATE_FAILED_ONLY",
            Self::RejectDuplicate => "WORKFLOW_ID_REUSE_POLICY_REJECT_DUPLICATE",
            Self::TerminateIfRunning => "WORKFLOW_ID_REUSE_POLICY_TERMINATE_IF_RUNNING",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for WorkflowIdReusePolicy {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "WORKFLOW_ID_REUSE_POLICY_UNSPECIFIED",
            "WORKFLOW_ID_REUSE_POLICY_ALLOW_DUPLICATE",
            "WORKFLOW_ID_REUSE_POLICY_ALLOW_DUPLICATE_FAILED_ONLY",
            "WORKFLOW_ID_REUSE_POLICY_REJECT_DUPLICATE",
            "WORKFLOW_ID_REUSE_POLICY_TERMINATE_IF_RUNNING",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = WorkflowIdReusePolicy;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "WORKFLOW_ID_REUSE_POLICY_UNSPECIFIED" => Ok(WorkflowIdReusePolicy::Unspecified),
                    "WORKFLOW_ID_REUSE_POLICY_ALLOW_DUPLICATE" => Ok(WorkflowIdReusePolicy::AllowDuplicate),
                    "WORKFLOW_ID_REUSE_POLICY_ALLOW_DUPLICATE_FAILED_ONLY" => Ok(WorkflowIdReusePolicy::AllowDuplicateFailedOnly),
                    "WORKFLOW_ID_REUSE_POLICY_REJECT_DUPLICATE" => Ok(WorkflowIdReusePolicy::RejectDuplicate),
                    "WORKFLOW_ID_REUSE_POLICY_TERMINATE_IF_RUNNING" => Ok(WorkflowIdReusePolicy::TerminateIfRunning),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for WorkflowRuleActionScope {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "WORKFLOW_RULE_ACTION_SCOPE_UNSPECIFIED",
            Self::Workflow => "WORKFLOW_RULE_ACTION_SCOPE_WORKFLOW",
            Self::Activity => "WORKFLOW_RULE_ACTION_SCOPE_ACTIVITY",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for WorkflowRuleActionScope {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "WORKFLOW_RULE_ACTION_SCOPE_UNSPECIFIED",
            "WORKFLOW_RULE_ACTION_SCOPE_WORKFLOW",
            "WORKFLOW_RULE_ACTION_SCOPE_ACTIVITY",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = WorkflowRuleActionScope;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "WORKFLOW_RULE_ACTION_SCOPE_UNSPECIFIED" => Ok(WorkflowRuleActionScope::Unspecified),
                    "WORKFLOW_RULE_ACTION_SCOPE_WORKFLOW" => Ok(WorkflowRuleActionScope::Workflow),
                    "WORKFLOW_RULE_ACTION_SCOPE_ACTIVITY" => Ok(WorkflowRuleActionScope::Activity),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for WorkflowTaskFailedCause {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "WORKFLOW_TASK_FAILED_CAUSE_UNSPECIFIED",
            Self::UnhandledCommand => "WORKFLOW_TASK_FAILED_CAUSE_UNHANDLED_COMMAND",
            Self::BadScheduleActivityAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_SCHEDULE_ACTIVITY_ATTRIBUTES",
            Self::BadRequestCancelActivityAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_REQUEST_CANCEL_ACTIVITY_ATTRIBUTES",
            Self::BadStartTimerAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_START_TIMER_ATTRIBUTES",
            Self::BadCancelTimerAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_CANCEL_TIMER_ATTRIBUTES",
            Self::BadRecordMarkerAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_RECORD_MARKER_ATTRIBUTES",
            Self::BadCompleteWorkflowExecutionAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_COMPLETE_WORKFLOW_EXECUTION_ATTRIBUTES",
            Self::BadFailWorkflowExecutionAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_FAIL_WORKFLOW_EXECUTION_ATTRIBUTES",
            Self::BadCancelWorkflowExecutionAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_CANCEL_WORKFLOW_EXECUTION_ATTRIBUTES",
            Self::BadRequestCancelExternalWorkflowExecutionAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_REQUEST_CANCEL_EXTERNAL_WORKFLOW_EXECUTION_ATTRIBUTES",
            Self::BadContinueAsNewAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_CONTINUE_AS_NEW_ATTRIBUTES",
            Self::StartTimerDuplicateId => "WORKFLOW_TASK_FAILED_CAUSE_START_TIMER_DUPLICATE_ID",
            Self::ResetStickyTaskQueue => "WORKFLOW_TASK_FAILED_CAUSE_RESET_STICKY_TASK_QUEUE",
            Self::WorkflowWorkerUnhandledFailure => "WORKFLOW_TASK_FAILED_CAUSE_WORKFLOW_WORKER_UNHANDLED_FAILURE",
            Self::BadSignalWorkflowExecutionAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_SIGNAL_WORKFLOW_EXECUTION_ATTRIBUTES",
            Self::BadStartChildExecutionAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_START_CHILD_EXECUTION_ATTRIBUTES",
            Self::ForceCloseCommand => "WORKFLOW_TASK_FAILED_CAUSE_FORCE_CLOSE_COMMAND",
            Self::FailoverCloseCommand => "WORKFLOW_TASK_FAILED_CAUSE_FAILOVER_CLOSE_COMMAND",
            Self::BadSignalInputSize => "WORKFLOW_TASK_FAILED_CAUSE_BAD_SIGNAL_INPUT_SIZE",
            Self::ResetWorkflow => "WORKFLOW_TASK_FAILED_CAUSE_RESET_WORKFLOW",
            Self::BadBinary => "WORKFLOW_TASK_FAILED_CAUSE_BAD_BINARY",
            Self::ScheduleActivityDuplicateId => "WORKFLOW_TASK_FAILED_CAUSE_SCHEDULE_ACTIVITY_DUPLICATE_ID",
            Self::BadSearchAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_SEARCH_ATTRIBUTES",
            Self::NonDeterministicError => "WORKFLOW_TASK_FAILED_CAUSE_NON_DETERMINISTIC_ERROR",
            Self::BadModifyWorkflowPropertiesAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_MODIFY_WORKFLOW_PROPERTIES_ATTRIBUTES",
            Self::PendingChildWorkflowsLimitExceeded => "WORKFLOW_TASK_FAILED_CAUSE_PENDING_CHILD_WORKFLOWS_LIMIT_EXCEEDED",
            Self::PendingActivitiesLimitExceeded => "WORKFLOW_TASK_FAILED_CAUSE_PENDING_ACTIVITIES_LIMIT_EXCEEDED",
            Self::PendingSignalsLimitExceeded => "WORKFLOW_TASK_FAILED_CAUSE_PENDING_SIGNALS_LIMIT_EXCEEDED",
            Self::PendingRequestCancelLimitExceeded => "WORKFLOW_TASK_FAILED_CAUSE_PENDING_REQUEST_CANCEL_LIMIT_EXCEEDED",
            Self::BadUpdateWorkflowExecutionMessage => "WORKFLOW_TASK_FAILED_CAUSE_BAD_UPDATE_WORKFLOW_EXECUTION_MESSAGE",
            Self::UnhandledUpdate => "WORKFLOW_TASK_FAILED_CAUSE_UNHANDLED_UPDATE",
            Self::BadScheduleNexusOperationAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_SCHEDULE_NEXUS_OPERATION_ATTRIBUTES",
            Self::PendingNexusOperationsLimitExceeded => "WORKFLOW_TASK_FAILED_CAUSE_PENDING_NEXUS_OPERATIONS_LIMIT_EXCEEDED",
            Self::BadRequestCancelNexusOperationAttributes => "WORKFLOW_TASK_FAILED_CAUSE_BAD_REQUEST_CANCEL_NEXUS_OPERATION_ATTRIBUTES",
            Self::FeatureDisabled => "WORKFLOW_TASK_FAILED_CAUSE_FEATURE_DISABLED",
            Self::GrpcMessageTooLarge => "WORKFLOW_TASK_FAILED_CAUSE_GRPC_MESSAGE_TOO_LARGE",
            Self::PayloadsTooLarge => "WORKFLOW_TASK_FAILED_CAUSE_PAYLOADS_TOO_LARGE",
            Self::ExternalStorageFailure => "WORKFLOW_TASK_FAILED_CAUSE_EXTERNAL_STORAGE_FAILURE",
            Self::WorkflowPauseRequestedBeforeTaskStarted => "WORKFLOW_TASK_FAILED_CAUSE_WORKFLOW_PAUSE_REQUESTED_BEFORE_TASK_STARTED",
            Self::RequestTooLarge => "WORKFLOW_TASK_FAILED_CAUSE_REQUEST_TOO_LARGE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for WorkflowTaskFailedCause {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "WORKFLOW_TASK_FAILED_CAUSE_UNSPECIFIED",
            "WORKFLOW_TASK_FAILED_CAUSE_UNHANDLED_COMMAND",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_SCHEDULE_ACTIVITY_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_REQUEST_CANCEL_ACTIVITY_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_START_TIMER_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_CANCEL_TIMER_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_RECORD_MARKER_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_COMPLETE_WORKFLOW_EXECUTION_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_FAIL_WORKFLOW_EXECUTION_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_CANCEL_WORKFLOW_EXECUTION_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_REQUEST_CANCEL_EXTERNAL_WORKFLOW_EXECUTION_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_CONTINUE_AS_NEW_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_START_TIMER_DUPLICATE_ID",
            "WORKFLOW_TASK_FAILED_CAUSE_RESET_STICKY_TASK_QUEUE",
            "WORKFLOW_TASK_FAILED_CAUSE_WORKFLOW_WORKER_UNHANDLED_FAILURE",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_SIGNAL_WORKFLOW_EXECUTION_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_START_CHILD_EXECUTION_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_FORCE_CLOSE_COMMAND",
            "WORKFLOW_TASK_FAILED_CAUSE_FAILOVER_CLOSE_COMMAND",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_SIGNAL_INPUT_SIZE",
            "WORKFLOW_TASK_FAILED_CAUSE_RESET_WORKFLOW",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_BINARY",
            "WORKFLOW_TASK_FAILED_CAUSE_SCHEDULE_ACTIVITY_DUPLICATE_ID",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_SEARCH_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_NON_DETERMINISTIC_ERROR",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_MODIFY_WORKFLOW_PROPERTIES_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_PENDING_CHILD_WORKFLOWS_LIMIT_EXCEEDED",
            "WORKFLOW_TASK_FAILED_CAUSE_PENDING_ACTIVITIES_LIMIT_EXCEEDED",
            "WORKFLOW_TASK_FAILED_CAUSE_PENDING_SIGNALS_LIMIT_EXCEEDED",
            "WORKFLOW_TASK_FAILED_CAUSE_PENDING_REQUEST_CANCEL_LIMIT_EXCEEDED",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_UPDATE_WORKFLOW_EXECUTION_MESSAGE",
            "WORKFLOW_TASK_FAILED_CAUSE_UNHANDLED_UPDATE",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_SCHEDULE_NEXUS_OPERATION_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_PENDING_NEXUS_OPERATIONS_LIMIT_EXCEEDED",
            "WORKFLOW_TASK_FAILED_CAUSE_BAD_REQUEST_CANCEL_NEXUS_OPERATION_ATTRIBUTES",
            "WORKFLOW_TASK_FAILED_CAUSE_FEATURE_DISABLED",
            "WORKFLOW_TASK_FAILED_CAUSE_GRPC_MESSAGE_TOO_LARGE",
            "WORKFLOW_TASK_FAILED_CAUSE_PAYLOADS_TOO_LARGE",
            "WORKFLOW_TASK_FAILED_CAUSE_EXTERNAL_STORAGE_FAILURE",
            "WORKFLOW_TASK_FAILED_CAUSE_WORKFLOW_PAUSE_REQUESTED_BEFORE_TASK_STARTED",
            "WORKFLOW_TASK_FAILED_CAUSE_REQUEST_TOO_LARGE",
        ];

        struct GeneratedVisitor;

        impl serde::de::Visitor<'_> for GeneratedVisitor {
            type Value = WorkflowTaskFailedCause;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "WORKFLOW_TASK_FAILED_CAUSE_UNSPECIFIED" => Ok(WorkflowTaskFailedCause::Unspecified),
                    "WORKFLOW_TASK_FAILED_CAUSE_UNHANDLED_COMMAND" => Ok(WorkflowTaskFailedCause::UnhandledCommand),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_SCHEDULE_ACTIVITY_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadScheduleActivityAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_REQUEST_CANCEL_ACTIVITY_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadRequestCancelActivityAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_START_TIMER_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadStartTimerAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_CANCEL_TIMER_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadCancelTimerAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_RECORD_MARKER_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadRecordMarkerAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_COMPLETE_WORKFLOW_EXECUTION_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadCompleteWorkflowExecutionAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_FAIL_WORKFLOW_EXECUTION_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadFailWorkflowExecutionAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_CANCEL_WORKFLOW_EXECUTION_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadCancelWorkflowExecutionAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_REQUEST_CANCEL_EXTERNAL_WORKFLOW_EXECUTION_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadRequestCancelExternalWorkflowExecutionAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_CONTINUE_AS_NEW_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadContinueAsNewAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_START_TIMER_DUPLICATE_ID" => Ok(WorkflowTaskFailedCause::StartTimerDuplicateId),
                    "WORKFLOW_TASK_FAILED_CAUSE_RESET_STICKY_TASK_QUEUE" => Ok(WorkflowTaskFailedCause::ResetStickyTaskQueue),
                    "WORKFLOW_TASK_FAILED_CAUSE_WORKFLOW_WORKER_UNHANDLED_FAILURE" => Ok(WorkflowTaskFailedCause::WorkflowWorkerUnhandledFailure),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_SIGNAL_WORKFLOW_EXECUTION_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadSignalWorkflowExecutionAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_START_CHILD_EXECUTION_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadStartChildExecutionAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_FORCE_CLOSE_COMMAND" => Ok(WorkflowTaskFailedCause::ForceCloseCommand),
                    "WORKFLOW_TASK_FAILED_CAUSE_FAILOVER_CLOSE_COMMAND" => Ok(WorkflowTaskFailedCause::FailoverCloseCommand),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_SIGNAL_INPUT_SIZE" => Ok(WorkflowTaskFailedCause::BadSignalInputSize),
                    "WORKFLOW_TASK_FAILED_CAUSE_RESET_WORKFLOW" => Ok(WorkflowTaskFailedCause::ResetWorkflow),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_BINARY" => Ok(WorkflowTaskFailedCause::BadBinary),
                    "WORKFLOW_TASK_FAILED_CAUSE_SCHEDULE_ACTIVITY_DUPLICATE_ID" => Ok(WorkflowTaskFailedCause::ScheduleActivityDuplicateId),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_SEARCH_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadSearchAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_NON_DETERMINISTIC_ERROR" => Ok(WorkflowTaskFailedCause::NonDeterministicError),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_MODIFY_WORKFLOW_PROPERTIES_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadModifyWorkflowPropertiesAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_PENDING_CHILD_WORKFLOWS_LIMIT_EXCEEDED" => Ok(WorkflowTaskFailedCause::PendingChildWorkflowsLimitExceeded),
                    "WORKFLOW_TASK_FAILED_CAUSE_PENDING_ACTIVITIES_LIMIT_EXCEEDED" => Ok(WorkflowTaskFailedCause::PendingActivitiesLimitExceeded),
                    "WORKFLOW_TASK_FAILED_CAUSE_PENDING_SIGNALS_LIMIT_EXCEEDED" => Ok(WorkflowTaskFailedCause::PendingSignalsLimitExceeded),
                    "WORKFLOW_TASK_FAILED_CAUSE_PENDING_REQUEST_CANCEL_LIMIT_EXCEEDED" => Ok(WorkflowTaskFailedCause::PendingRequestCancelLimitExceeded),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_UPDATE_WORKFLOW_EXECUTION_MESSAGE" => Ok(WorkflowTaskFailedCause::BadUpdateWorkflowExecutionMessage),
                    "WORKFLOW_TASK_FAILED_CAUSE_UNHANDLED_UPDATE" => Ok(WorkflowTaskFailedCause::UnhandledUpdate),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_SCHEDULE_NEXUS_OPERATION_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadScheduleNexusOperationAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_PENDING_NEXUS_OPERATIONS_LIMIT_EXCEEDED" => Ok(WorkflowTaskFailedCause::PendingNexusOperationsLimitExceeded),
                    "WORKFLOW_TASK_FAILED_CAUSE_BAD_REQUEST_CANCEL_NEXUS_OPERATION_ATTRIBUTES" => Ok(WorkflowTaskFailedCause::BadRequestCancelNexusOperationAttributes),
                    "WORKFLOW_TASK_FAILED_CAUSE_FEATURE_DISABLED" => Ok(WorkflowTaskFailedCause::FeatureDisabled),
                    "WORKFLOW_TASK_FAILED_CAUSE_GRPC_MESSAGE_TOO_LARGE" => Ok(WorkflowTaskFailedCause::GrpcMessageTooLarge),
                    "WORKFLOW_TASK_FAILED_CAUSE_PAYLOADS_TOO_LARGE" => Ok(WorkflowTaskFailedCause::PayloadsTooLarge),
                    "WORKFLOW_TASK_FAILED_CAUSE_EXTERNAL_STORAGE_FAILURE" => Ok(WorkflowTaskFailedCause::ExternalStorageFailure),
                    "WORKFLOW_TASK_FAILED_CAUSE_WORKFLOW_PAUSE_REQUESTED_BEFORE_TASK_STARTED" => Ok(WorkflowTaskFailedCause::WorkflowPauseRequestedBeforeTaskStarted),
                    "WORKFLOW_TASK_FAILED_CAUSE_REQUEST_TOO_LARGE" => Ok(WorkflowTaskFailedCause::RequestTooLarge),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
