impl serde::Serialize for ActivityFailureInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.scheduled_event_id != 0 {
            len += 1;
        }
        if self.started_event_id != 0 {
            len += 1;
        }
        if !self.identity.is_empty() {
            len += 1;
        }
        if self.activity_type.is_some() {
            len += 1;
        }
        if !self.activity_id.is_empty() {
            len += 1;
        }
        if self.retry_state != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.failure.v1.ActivityFailureInfo", len)?;
        if self.scheduled_event_id != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("scheduledEventId", ToString::to_string(&self.scheduled_event_id).as_str())?;
        }
        if self.started_event_id != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("startedEventId", ToString::to_string(&self.started_event_id).as_str())?;
        }
        if !self.identity.is_empty() {
            struct_ser.serialize_field("identity", &self.identity)?;
        }
        if let Some(v) = self.activity_type.as_ref() {
            struct_ser.serialize_field("activityType", v)?;
        }
        if !self.activity_id.is_empty() {
            struct_ser.serialize_field("activityId", &self.activity_id)?;
        }
        if self.retry_state != 0 {
            let v = super::super::enums::v1::RetryState::try_from(self.retry_state)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.retry_state)))?;
            struct_ser.serialize_field("retryState", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ActivityFailureInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "scheduled_event_id",
            "scheduledEventId",
            "started_event_id",
            "startedEventId",
            "identity",
            "activity_type",
            "activityType",
            "activity_id",
            "activityId",
            "retry_state",
            "retryState",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ScheduledEventId,
            StartedEventId,
            Identity,
            ActivityType,
            ActivityId,
            RetryState,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl serde::de::Visitor<'_> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "scheduledEventId" | "scheduled_event_id" => Ok(GeneratedField::ScheduledEventId),
                            "startedEventId" | "started_event_id" => Ok(GeneratedField::StartedEventId),
                            "identity" => Ok(GeneratedField::Identity),
                            "activityType" | "activity_type" => Ok(GeneratedField::ActivityType),
                            "activityId" | "activity_id" => Ok(GeneratedField::ActivityId),
                            "retryState" | "retry_state" => Ok(GeneratedField::RetryState),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ActivityFailureInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.failure.v1.ActivityFailureInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ActivityFailureInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut scheduled_event_id__ = None;
                let mut started_event_id__ = None;
                let mut identity__ = None;
                let mut activity_type__ = None;
                let mut activity_id__ = None;
                let mut retry_state__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ScheduledEventId => {
                            if scheduled_event_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("scheduledEventId"));
                            }
                            scheduled_event_id__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::StartedEventId => {
                            if started_event_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startedEventId"));
                            }
                            started_event_id__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Identity => {
                            if identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identity"));
                            }
                            identity__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ActivityType => {
                            if activity_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("activityType"));
                            }
                            activity_type__ = map_.next_value()?;
                        }
                        GeneratedField::ActivityId => {
                            if activity_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("activityId"));
                            }
                            activity_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RetryState => {
                            if retry_state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("retryState"));
                            }
                            retry_state__ = Some(map_.next_value::<super::super::enums::v1::RetryState>()? as i32);
                        }
                    }
                }
                Ok(ActivityFailureInfo {
                    scheduled_event_id: scheduled_event_id__.unwrap_or_default(),
                    started_event_id: started_event_id__.unwrap_or_default(),
                    identity: identity__.unwrap_or_default(),
                    activity_type: activity_type__,
                    activity_id: activity_id__.unwrap_or_default(),
                    retry_state: retry_state__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.failure.v1.ActivityFailureInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ApplicationFailureInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.r#type.is_empty() {
            len += 1;
        }
        if self.non_retryable {
            len += 1;
        }
        if self.details.is_some() {
            len += 1;
        }
        if self.next_retry_delay.is_some() {
            len += 1;
        }
        if self.category != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.failure.v1.ApplicationFailureInfo", len)?;
        if !self.r#type.is_empty() {
            struct_ser.serialize_field("type", &self.r#type)?;
        }
        if self.non_retryable {
            struct_ser.serialize_field("nonRetryable", &self.non_retryable)?;
        }
        if let Some(v) = self.details.as_ref() {
            struct_ser.serialize_field("details", v)?;
        }
        if let Some(v) = self.next_retry_delay.as_ref() {
            struct_ser.serialize_field("nextRetryDelay", v)?;
        }
        if self.category != 0 {
            let v = super::super::enums::v1::ApplicationErrorCategory::try_from(self.category)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.category)))?;
            struct_ser.serialize_field("category", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ApplicationFailureInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "type",
            "non_retryable",
            "nonRetryable",
            "details",
            "next_retry_delay",
            "nextRetryDelay",
            "category",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Type,
            NonRetryable,
            Details,
            NextRetryDelay,
            Category,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl serde::de::Visitor<'_> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "type" => Ok(GeneratedField::Type),
                            "nonRetryable" | "non_retryable" => Ok(GeneratedField::NonRetryable),
                            "details" => Ok(GeneratedField::Details),
                            "nextRetryDelay" | "next_retry_delay" => Ok(GeneratedField::NextRetryDelay),
                            "category" => Ok(GeneratedField::Category),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ApplicationFailureInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.failure.v1.ApplicationFailureInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ApplicationFailureInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut r#type__ = None;
                let mut non_retryable__ = None;
                let mut details__ = None;
                let mut next_retry_delay__ = None;
                let mut category__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Type => {
                            if r#type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("type"));
                            }
                            r#type__ = Some(map_.next_value()?);
                        }
                        GeneratedField::NonRetryable => {
                            if non_retryable__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nonRetryable"));
                            }
                            non_retryable__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Details => {
                            if details__.is_some() {
                                return Err(serde::de::Error::duplicate_field("details"));
                            }
                            details__ = map_.next_value()?;
                        }
                        GeneratedField::NextRetryDelay => {
                            if next_retry_delay__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nextRetryDelay"));
                            }
                            next_retry_delay__ = map_.next_value()?;
                        }
                        GeneratedField::Category => {
                            if category__.is_some() {
                                return Err(serde::de::Error::duplicate_field("category"));
                            }
                            category__ = Some(map_.next_value::<super::super::enums::v1::ApplicationErrorCategory>()? as i32);
                        }
                    }
                }
                Ok(ApplicationFailureInfo {
                    r#type: r#type__.unwrap_or_default(),
                    non_retryable: non_retryable__.unwrap_or_default(),
                    details: details__,
                    next_retry_delay: next_retry_delay__,
                    category: category__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.failure.v1.ApplicationFailureInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for CanceledFailureInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.details.is_some() {
            len += 1;
        }
        if !self.identity.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.failure.v1.CanceledFailureInfo", len)?;
        if let Some(v) = self.details.as_ref() {
            struct_ser.serialize_field("details", v)?;
        }
        if !self.identity.is_empty() {
            struct_ser.serialize_field("identity", &self.identity)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CanceledFailureInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "details",
            "identity",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Details,
            Identity,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl serde::de::Visitor<'_> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "details" => Ok(GeneratedField::Details),
                            "identity" => Ok(GeneratedField::Identity),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CanceledFailureInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.failure.v1.CanceledFailureInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<CanceledFailureInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut details__ = None;
                let mut identity__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Details => {
                            if details__.is_some() {
                                return Err(serde::de::Error::duplicate_field("details"));
                            }
                            details__ = map_.next_value()?;
                        }
                        GeneratedField::Identity => {
                            if identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identity"));
                            }
                            identity__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(CanceledFailureInfo {
                    details: details__,
                    identity: identity__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.failure.v1.CanceledFailureInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ChildWorkflowExecutionFailureInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.namespace.is_empty() {
            len += 1;
        }
        if self.workflow_execution.is_some() {
            len += 1;
        }
        if self.workflow_type.is_some() {
            len += 1;
        }
        if self.initiated_event_id != 0 {
            len += 1;
        }
        if self.started_event_id != 0 {
            len += 1;
        }
        if self.retry_state != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.failure.v1.ChildWorkflowExecutionFailureInfo", len)?;
        if !self.namespace.is_empty() {
            struct_ser.serialize_field("namespace", &self.namespace)?;
        }
        if let Some(v) = self.workflow_execution.as_ref() {
            struct_ser.serialize_field("workflowExecution", v)?;
        }
        if let Some(v) = self.workflow_type.as_ref() {
            struct_ser.serialize_field("workflowType", v)?;
        }
        if self.initiated_event_id != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("initiatedEventId", ToString::to_string(&self.initiated_event_id).as_str())?;
        }
        if self.started_event_id != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("startedEventId", ToString::to_string(&self.started_event_id).as_str())?;
        }
        if self.retry_state != 0 {
            let v = super::super::enums::v1::RetryState::try_from(self.retry_state)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.retry_state)))?;
            struct_ser.serialize_field("retryState", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ChildWorkflowExecutionFailureInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "namespace",
            "workflow_execution",
            "workflowExecution",
            "workflow_type",
            "workflowType",
            "initiated_event_id",
            "initiatedEventId",
            "started_event_id",
            "startedEventId",
            "retry_state",
            "retryState",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Namespace,
            WorkflowExecution,
            WorkflowType,
            InitiatedEventId,
            StartedEventId,
            RetryState,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl serde::de::Visitor<'_> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "namespace" => Ok(GeneratedField::Namespace),
                            "workflowExecution" | "workflow_execution" => Ok(GeneratedField::WorkflowExecution),
                            "workflowType" | "workflow_type" => Ok(GeneratedField::WorkflowType),
                            "initiatedEventId" | "initiated_event_id" => Ok(GeneratedField::InitiatedEventId),
                            "startedEventId" | "started_event_id" => Ok(GeneratedField::StartedEventId),
                            "retryState" | "retry_state" => Ok(GeneratedField::RetryState),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ChildWorkflowExecutionFailureInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.failure.v1.ChildWorkflowExecutionFailureInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ChildWorkflowExecutionFailureInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut namespace__ = None;
                let mut workflow_execution__ = None;
                let mut workflow_type__ = None;
                let mut initiated_event_id__ = None;
                let mut started_event_id__ = None;
                let mut retry_state__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Namespace => {
                            if namespace__.is_some() {
                                return Err(serde::de::Error::duplicate_field("namespace"));
                            }
                            namespace__ = Some(map_.next_value()?);
                        }
                        GeneratedField::WorkflowExecution => {
                            if workflow_execution__.is_some() {
                                return Err(serde::de::Error::duplicate_field("workflowExecution"));
                            }
                            workflow_execution__ = map_.next_value()?;
                        }
                        GeneratedField::WorkflowType => {
                            if workflow_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("workflowType"));
                            }
                            workflow_type__ = map_.next_value()?;
                        }
                        GeneratedField::InitiatedEventId => {
                            if initiated_event_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("initiatedEventId"));
                            }
                            initiated_event_id__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::StartedEventId => {
                            if started_event_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startedEventId"));
                            }
                            started_event_id__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::RetryState => {
                            if retry_state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("retryState"));
                            }
                            retry_state__ = Some(map_.next_value::<super::super::enums::v1::RetryState>()? as i32);
                        }
                    }
                }
                Ok(ChildWorkflowExecutionFailureInfo {
                    namespace: namespace__.unwrap_or_default(),
                    workflow_execution: workflow_execution__,
                    workflow_type: workflow_type__,
                    initiated_event_id: initiated_event_id__.unwrap_or_default(),
                    started_event_id: started_event_id__.unwrap_or_default(),
                    retry_state: retry_state__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.failure.v1.ChildWorkflowExecutionFailureInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Failure {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.message.is_empty() {
            len += 1;
        }
        if !self.source.is_empty() {
            len += 1;
        }
        if !self.stack_trace.is_empty() {
            len += 1;
        }
        if self.encoded_attributes.is_some() {
            len += 1;
        }
        if self.cause.is_some() {
            len += 1;
        }
        if self.failure_info.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.failure.v1.Failure", len)?;
        if !self.message.is_empty() {
            struct_ser.serialize_field("message", &self.message)?;
        }
        if !self.source.is_empty() {
            struct_ser.serialize_field("source", &self.source)?;
        }
        if !self.stack_trace.is_empty() {
            struct_ser.serialize_field("stackTrace", &self.stack_trace)?;
        }
        if let Some(v) = self.encoded_attributes.as_ref() {
            struct_ser.serialize_field("encodedAttributes", v)?;
        }
        if let Some(v) = self.cause.as_ref() {
            struct_ser.serialize_field("cause", v)?;
        }
        if let Some(v) = self.failure_info.as_ref() {
            match v {
                failure::FailureInfo::ApplicationFailureInfo(v) => {
                    struct_ser.serialize_field("applicationFailureInfo", v)?;
                }
                failure::FailureInfo::TimeoutFailureInfo(v) => {
                    struct_ser.serialize_field("timeoutFailureInfo", v)?;
                }
                failure::FailureInfo::CanceledFailureInfo(v) => {
                    struct_ser.serialize_field("canceledFailureInfo", v)?;
                }
                failure::FailureInfo::TerminatedFailureInfo(v) => {
                    struct_ser.serialize_field("terminatedFailureInfo", v)?;
                }
                failure::FailureInfo::ServerFailureInfo(v) => {
                    struct_ser.serialize_field("serverFailureInfo", v)?;
                }
                failure::FailureInfo::ResetWorkflowFailureInfo(v) => {
                    struct_ser.serialize_field("resetWorkflowFailureInfo", v)?;
                }
                failure::FailureInfo::ActivityFailureInfo(v) => {
                    struct_ser.serialize_field("activityFailureInfo", v)?;
                }
                failure::FailureInfo::ChildWorkflowExecutionFailureInfo(v) => {
                    struct_ser.serialize_field("childWorkflowExecutionFailureInfo", v)?;
                }
                failure::FailureInfo::NexusOperationExecutionFailureInfo(v) => {
                    struct_ser.serialize_field("nexusOperationExecutionFailureInfo", v)?;
                }
                failure::FailureInfo::NexusHandlerFailureInfo(v) => {
                    struct_ser.serialize_field("nexusHandlerFailureInfo", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Failure {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "message",
            "source",
            "stack_trace",
            "stackTrace",
            "encoded_attributes",
            "encodedAttributes",
            "cause",
            "application_failure_info",
            "applicationFailureInfo",
            "timeout_failure_info",
            "timeoutFailureInfo",
            "canceled_failure_info",
            "canceledFailureInfo",
            "terminated_failure_info",
            "terminatedFailureInfo",
            "server_failure_info",
            "serverFailureInfo",
            "reset_workflow_failure_info",
            "resetWorkflowFailureInfo",
            "activity_failure_info",
            "activityFailureInfo",
            "child_workflow_execution_failure_info",
            "childWorkflowExecutionFailureInfo",
            "nexus_operation_execution_failure_info",
            "nexusOperationExecutionFailureInfo",
            "nexus_handler_failure_info",
            "nexusHandlerFailureInfo",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Message,
            Source,
            StackTrace,
            EncodedAttributes,
            Cause,
            ApplicationFailureInfo,
            TimeoutFailureInfo,
            CanceledFailureInfo,
            TerminatedFailureInfo,
            ServerFailureInfo,
            ResetWorkflowFailureInfo,
            ActivityFailureInfo,
            ChildWorkflowExecutionFailureInfo,
            NexusOperationExecutionFailureInfo,
            NexusHandlerFailureInfo,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl serde::de::Visitor<'_> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "message" => Ok(GeneratedField::Message),
                            "source" => Ok(GeneratedField::Source),
                            "stackTrace" | "stack_trace" => Ok(GeneratedField::StackTrace),
                            "encodedAttributes" | "encoded_attributes" => Ok(GeneratedField::EncodedAttributes),
                            "cause" => Ok(GeneratedField::Cause),
                            "applicationFailureInfo" | "application_failure_info" => Ok(GeneratedField::ApplicationFailureInfo),
                            "timeoutFailureInfo" | "timeout_failure_info" => Ok(GeneratedField::TimeoutFailureInfo),
                            "canceledFailureInfo" | "canceled_failure_info" => Ok(GeneratedField::CanceledFailureInfo),
                            "terminatedFailureInfo" | "terminated_failure_info" => Ok(GeneratedField::TerminatedFailureInfo),
                            "serverFailureInfo" | "server_failure_info" => Ok(GeneratedField::ServerFailureInfo),
                            "resetWorkflowFailureInfo" | "reset_workflow_failure_info" => Ok(GeneratedField::ResetWorkflowFailureInfo),
                            "activityFailureInfo" | "activity_failure_info" => Ok(GeneratedField::ActivityFailureInfo),
                            "childWorkflowExecutionFailureInfo" | "child_workflow_execution_failure_info" => Ok(GeneratedField::ChildWorkflowExecutionFailureInfo),
                            "nexusOperationExecutionFailureInfo" | "nexus_operation_execution_failure_info" => Ok(GeneratedField::NexusOperationExecutionFailureInfo),
                            "nexusHandlerFailureInfo" | "nexus_handler_failure_info" => Ok(GeneratedField::NexusHandlerFailureInfo),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Failure;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.failure.v1.Failure")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Failure, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut message__ = None;
                let mut source__ = None;
                let mut stack_trace__ = None;
                let mut encoded_attributes__ = None;
                let mut cause__ = None;
                let mut failure_info__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Message => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("message"));
                            }
                            message__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Source => {
                            if source__.is_some() {
                                return Err(serde::de::Error::duplicate_field("source"));
                            }
                            source__ = Some(map_.next_value()?);
                        }
                        GeneratedField::StackTrace => {
                            if stack_trace__.is_some() {
                                return Err(serde::de::Error::duplicate_field("stackTrace"));
                            }
                            stack_trace__ = Some(map_.next_value()?);
                        }
                        GeneratedField::EncodedAttributes => {
                            if encoded_attributes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("encodedAttributes"));
                            }
                            encoded_attributes__ = map_.next_value()?;
                        }
                        GeneratedField::Cause => {
                            if cause__.is_some() {
                                return Err(serde::de::Error::duplicate_field("cause"));
                            }
                            cause__ = map_.next_value()?;
                        }
                        GeneratedField::ApplicationFailureInfo => {
                            if failure_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("applicationFailureInfo"));
                            }
                            failure_info__ = map_.next_value::<::std::option::Option<_>>()?.map(failure::FailureInfo::ApplicationFailureInfo)
;
                        }
                        GeneratedField::TimeoutFailureInfo => {
                            if failure_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("timeoutFailureInfo"));
                            }
                            failure_info__ = map_.next_value::<::std::option::Option<_>>()?.map(failure::FailureInfo::TimeoutFailureInfo)
;
                        }
                        GeneratedField::CanceledFailureInfo => {
                            if failure_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("canceledFailureInfo"));
                            }
                            failure_info__ = map_.next_value::<::std::option::Option<_>>()?.map(failure::FailureInfo::CanceledFailureInfo)
;
                        }
                        GeneratedField::TerminatedFailureInfo => {
                            if failure_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("terminatedFailureInfo"));
                            }
                            failure_info__ = map_.next_value::<::std::option::Option<_>>()?.map(failure::FailureInfo::TerminatedFailureInfo)
;
                        }
                        GeneratedField::ServerFailureInfo => {
                            if failure_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("serverFailureInfo"));
                            }
                            failure_info__ = map_.next_value::<::std::option::Option<_>>()?.map(failure::FailureInfo::ServerFailureInfo)
;
                        }
                        GeneratedField::ResetWorkflowFailureInfo => {
                            if failure_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("resetWorkflowFailureInfo"));
                            }
                            failure_info__ = map_.next_value::<::std::option::Option<_>>()?.map(failure::FailureInfo::ResetWorkflowFailureInfo)
;
                        }
                        GeneratedField::ActivityFailureInfo => {
                            if failure_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("activityFailureInfo"));
                            }
                            failure_info__ = map_.next_value::<::std::option::Option<_>>()?.map(failure::FailureInfo::ActivityFailureInfo)
;
                        }
                        GeneratedField::ChildWorkflowExecutionFailureInfo => {
                            if failure_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("childWorkflowExecutionFailureInfo"));
                            }
                            failure_info__ = map_.next_value::<::std::option::Option<_>>()?.map(failure::FailureInfo::ChildWorkflowExecutionFailureInfo)
;
                        }
                        GeneratedField::NexusOperationExecutionFailureInfo => {
                            if failure_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nexusOperationExecutionFailureInfo"));
                            }
                            failure_info__ = map_.next_value::<::std::option::Option<_>>()?.map(failure::FailureInfo::NexusOperationExecutionFailureInfo)
;
                        }
                        GeneratedField::NexusHandlerFailureInfo => {
                            if failure_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nexusHandlerFailureInfo"));
                            }
                            failure_info__ = map_.next_value::<::std::option::Option<_>>()?.map(failure::FailureInfo::NexusHandlerFailureInfo)
;
                        }
                    }
                }
                Ok(Failure {
                    message: message__.unwrap_or_default(),
                    source: source__.unwrap_or_default(),
                    stack_trace: stack_trace__.unwrap_or_default(),
                    encoded_attributes: encoded_attributes__,
                    cause: cause__,
                    failure_info: failure_info__,
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.failure.v1.Failure", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MultiOperationExecutionAborted {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("temporal.api.failure.v1.MultiOperationExecutionAborted", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MultiOperationExecutionAborted {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl serde::de::Visitor<'_> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Err(serde::de::Error::unknown_field(value, FIELDS))
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MultiOperationExecutionAborted;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.failure.v1.MultiOperationExecutionAborted")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MultiOperationExecutionAborted, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(MultiOperationExecutionAborted {
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.failure.v1.MultiOperationExecutionAborted", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NexusHandlerFailureInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.r#type.is_empty() {
            len += 1;
        }
        if self.retry_behavior != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.failure.v1.NexusHandlerFailureInfo", len)?;
        if !self.r#type.is_empty() {
            struct_ser.serialize_field("type", &self.r#type)?;
        }
        if self.retry_behavior != 0 {
            let v = super::super::enums::v1::NexusHandlerErrorRetryBehavior::try_from(self.retry_behavior)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.retry_behavior)))?;
            struct_ser.serialize_field("retryBehavior", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NexusHandlerFailureInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "type",
            "retry_behavior",
            "retryBehavior",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Type,
            RetryBehavior,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl serde::de::Visitor<'_> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "type" => Ok(GeneratedField::Type),
                            "retryBehavior" | "retry_behavior" => Ok(GeneratedField::RetryBehavior),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NexusHandlerFailureInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.failure.v1.NexusHandlerFailureInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NexusHandlerFailureInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut r#type__ = None;
                let mut retry_behavior__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Type => {
                            if r#type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("type"));
                            }
                            r#type__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RetryBehavior => {
                            if retry_behavior__.is_some() {
                                return Err(serde::de::Error::duplicate_field("retryBehavior"));
                            }
                            retry_behavior__ = Some(map_.next_value::<super::super::enums::v1::NexusHandlerErrorRetryBehavior>()? as i32);
                        }
                    }
                }
                Ok(NexusHandlerFailureInfo {
                    r#type: r#type__.unwrap_or_default(),
                    retry_behavior: retry_behavior__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.failure.v1.NexusHandlerFailureInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NexusOperationFailureInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.scheduled_event_id != 0 {
            len += 1;
        }
        if !self.endpoint.is_empty() {
            len += 1;
        }
        if !self.service.is_empty() {
            len += 1;
        }
        if !self.operation.is_empty() {
            len += 1;
        }
        if !self.operation_id.is_empty() {
            len += 1;
        }
        if !self.operation_token.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.failure.v1.NexusOperationFailureInfo", len)?;
        if self.scheduled_event_id != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("scheduledEventId", ToString::to_string(&self.scheduled_event_id).as_str())?;
        }
        if !self.endpoint.is_empty() {
            struct_ser.serialize_field("endpoint", &self.endpoint)?;
        }
        if !self.service.is_empty() {
            struct_ser.serialize_field("service", &self.service)?;
        }
        if !self.operation.is_empty() {
            struct_ser.serialize_field("operation", &self.operation)?;
        }
        if !self.operation_id.is_empty() {
            struct_ser.serialize_field("operationId", &self.operation_id)?;
        }
        if !self.operation_token.is_empty() {
            struct_ser.serialize_field("operationToken", &self.operation_token)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NexusOperationFailureInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "scheduled_event_id",
            "scheduledEventId",
            "endpoint",
            "service",
            "operation",
            "operation_id",
            "operationId",
            "operation_token",
            "operationToken",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ScheduledEventId,
            Endpoint,
            Service,
            Operation,
            OperationId,
            OperationToken,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl serde::de::Visitor<'_> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "scheduledEventId" | "scheduled_event_id" => Ok(GeneratedField::ScheduledEventId),
                            "endpoint" => Ok(GeneratedField::Endpoint),
                            "service" => Ok(GeneratedField::Service),
                            "operation" => Ok(GeneratedField::Operation),
                            "operationId" | "operation_id" => Ok(GeneratedField::OperationId),
                            "operationToken" | "operation_token" => Ok(GeneratedField::OperationToken),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NexusOperationFailureInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.failure.v1.NexusOperationFailureInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NexusOperationFailureInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut scheduled_event_id__ = None;
                let mut endpoint__ = None;
                let mut service__ = None;
                let mut operation__ = None;
                let mut operation_id__ = None;
                let mut operation_token__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ScheduledEventId => {
                            if scheduled_event_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("scheduledEventId"));
                            }
                            scheduled_event_id__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Endpoint => {
                            if endpoint__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endpoint"));
                            }
                            endpoint__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Service => {
                            if service__.is_some() {
                                return Err(serde::de::Error::duplicate_field("service"));
                            }
                            service__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Operation => {
                            if operation__.is_some() {
                                return Err(serde::de::Error::duplicate_field("operation"));
                            }
                            operation__ = Some(map_.next_value()?);
                        }
                        GeneratedField::OperationId => {
                            if operation_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("operationId"));
                            }
                            operation_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::OperationToken => {
                            if operation_token__.is_some() {
                                return Err(serde::de::Error::duplicate_field("operationToken"));
                            }
                            operation_token__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(NexusOperationFailureInfo {
                    scheduled_event_id: scheduled_event_id__.unwrap_or_default(),
                    endpoint: endpoint__.unwrap_or_default(),
                    service: service__.unwrap_or_default(),
                    operation: operation__.unwrap_or_default(),
                    operation_id: operation_id__.unwrap_or_default(),
                    operation_token: operation_token__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.failure.v1.NexusOperationFailureInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ResetWorkflowFailureInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.last_heartbeat_details.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.failure.v1.ResetWorkflowFailureInfo", len)?;
        if let Some(v) = self.last_heartbeat_details.as_ref() {
            struct_ser.serialize_field("lastHeartbeatDetails", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ResetWorkflowFailureInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "last_heartbeat_details",
            "lastHeartbeatDetails",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            LastHeartbeatDetails,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl serde::de::Visitor<'_> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "lastHeartbeatDetails" | "last_heartbeat_details" => Ok(GeneratedField::LastHeartbeatDetails),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ResetWorkflowFailureInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.failure.v1.ResetWorkflowFailureInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ResetWorkflowFailureInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut last_heartbeat_details__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::LastHeartbeatDetails => {
                            if last_heartbeat_details__.is_some() {
                                return Err(serde::de::Error::duplicate_field("lastHeartbeatDetails"));
                            }
                            last_heartbeat_details__ = map_.next_value()?;
                        }
                    }
                }
                Ok(ResetWorkflowFailureInfo {
                    last_heartbeat_details: last_heartbeat_details__,
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.failure.v1.ResetWorkflowFailureInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ServerFailureInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.non_retryable {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.failure.v1.ServerFailureInfo", len)?;
        if self.non_retryable {
            struct_ser.serialize_field("nonRetryable", &self.non_retryable)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ServerFailureInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "non_retryable",
            "nonRetryable",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NonRetryable,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl serde::de::Visitor<'_> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "nonRetryable" | "non_retryable" => Ok(GeneratedField::NonRetryable),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ServerFailureInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.failure.v1.ServerFailureInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ServerFailureInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut non_retryable__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NonRetryable => {
                            if non_retryable__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nonRetryable"));
                            }
                            non_retryable__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(ServerFailureInfo {
                    non_retryable: non_retryable__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.failure.v1.ServerFailureInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TerminatedFailureInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.identity.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.failure.v1.TerminatedFailureInfo", len)?;
        if !self.identity.is_empty() {
            struct_ser.serialize_field("identity", &self.identity)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TerminatedFailureInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "identity",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Identity,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl serde::de::Visitor<'_> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "identity" => Ok(GeneratedField::Identity),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TerminatedFailureInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.failure.v1.TerminatedFailureInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TerminatedFailureInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut identity__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Identity => {
                            if identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identity"));
                            }
                            identity__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(TerminatedFailureInfo {
                    identity: identity__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.failure.v1.TerminatedFailureInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TimeoutFailureInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.timeout_type != 0 {
            len += 1;
        }
        if self.last_heartbeat_details.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.failure.v1.TimeoutFailureInfo", len)?;
        if self.timeout_type != 0 {
            let v = super::super::enums::v1::TimeoutType::try_from(self.timeout_type)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.timeout_type)))?;
            struct_ser.serialize_field("timeoutType", &v)?;
        }
        if let Some(v) = self.last_heartbeat_details.as_ref() {
            struct_ser.serialize_field("lastHeartbeatDetails", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TimeoutFailureInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "timeout_type",
            "timeoutType",
            "last_heartbeat_details",
            "lastHeartbeatDetails",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TimeoutType,
            LastHeartbeatDetails,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl serde::de::Visitor<'_> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "timeoutType" | "timeout_type" => Ok(GeneratedField::TimeoutType),
                            "lastHeartbeatDetails" | "last_heartbeat_details" => Ok(GeneratedField::LastHeartbeatDetails),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TimeoutFailureInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.failure.v1.TimeoutFailureInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TimeoutFailureInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut timeout_type__ = None;
                let mut last_heartbeat_details__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::TimeoutType => {
                            if timeout_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("timeoutType"));
                            }
                            timeout_type__ = Some(map_.next_value::<super::super::enums::v1::TimeoutType>()? as i32);
                        }
                        GeneratedField::LastHeartbeatDetails => {
                            if last_heartbeat_details__.is_some() {
                                return Err(serde::de::Error::duplicate_field("lastHeartbeatDetails"));
                            }
                            last_heartbeat_details__ = map_.next_value()?;
                        }
                    }
                }
                Ok(TimeoutFailureInfo {
                    timeout_type: timeout_type__.unwrap_or_default(),
                    last_heartbeat_details: last_heartbeat_details__,
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.failure.v1.TimeoutFailureInfo", FIELDS, GeneratedVisitor)
    }
}
