impl serde::Serialize for ActivityType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.name.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.ActivityType", len)?;
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ActivityType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "name",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Name,
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
                            "name" => Ok(GeneratedField::Name),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ActivityType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.ActivityType")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ActivityType, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut name__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(ActivityType {
                    name: name__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.ActivityType", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Callback {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.links.is_empty() {
            len += 1;
        }
        if self.variant.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Callback", len)?;
        if !self.links.is_empty() {
            struct_ser.serialize_field("links", &self.links)?;
        }
        if let Some(v) = self.variant.as_ref() {
            match v {
                callback::Variant::Nexus(v) => {
                    struct_ser.serialize_field("nexus", v)?;
                }
                callback::Variant::Internal(v) => {
                    struct_ser.serialize_field("internal", v)?;
                }
                callback::Variant::NexusHandler(v) => {
                    struct_ser.serialize_field("nexusHandler", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Callback {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "links",
            "nexus",
            "internal",
            "nexus_handler",
            "nexusHandler",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Links,
            Nexus,
            Internal,
            NexusHandler,
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
                            "links" => Ok(GeneratedField::Links),
                            "nexus" => Ok(GeneratedField::Nexus),
                            "internal" => Ok(GeneratedField::Internal),
                            "nexusHandler" | "nexus_handler" => Ok(GeneratedField::NexusHandler),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Callback;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Callback")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Callback, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut links__ = None;
                let mut variant__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Links => {
                            if links__.is_some() {
                                return Err(serde::de::Error::duplicate_field("links"));
                            }
                            links__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Nexus => {
                            if variant__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nexus"));
                            }
                            variant__ = map_.next_value::<::std::option::Option<_>>()?.map(callback::Variant::Nexus)
;
                        }
                        GeneratedField::Internal => {
                            if variant__.is_some() {
                                return Err(serde::de::Error::duplicate_field("internal"));
                            }
                            variant__ = map_.next_value::<::std::option::Option<_>>()?.map(callback::Variant::Internal)
;
                        }
                        GeneratedField::NexusHandler => {
                            if variant__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nexusHandler"));
                            }
                            variant__ = map_.next_value::<::std::option::Option<_>>()?.map(callback::Variant::NexusHandler)
;
                        }
                    }
                }
                Ok(Callback {
                    links: links__.unwrap_or_default(),
                    variant: variant__,
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Callback", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for callback::Internal {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.data.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Callback.Internal", len)?;
        if !self.data.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("data", pbjson::private::base64::encode(&self.data).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for callback::Internal {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "data",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Data,
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
                            "data" => Ok(GeneratedField::Data),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = callback::Internal;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Callback.Internal")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<callback::Internal, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut data__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Data => {
                            if data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("data"));
                            }
                            data__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(callback::Internal {
                    data: data__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Callback.Internal", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for callback::Nexus {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.url.is_empty() {
            len += 1;
        }
        if !self.header.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Callback.Nexus", len)?;
        if !self.url.is_empty() {
            struct_ser.serialize_field("url", &self.url)?;
        }
        if !self.header.is_empty() {
            struct_ser.serialize_field("header", &self.header)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for callback::Nexus {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "url",
            "header",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Url,
            Header,
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
                            "url" => Ok(GeneratedField::Url),
                            "header" => Ok(GeneratedField::Header),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = callback::Nexus;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Callback.Nexus")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<callback::Nexus, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut url__ = None;
                let mut header__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Url => {
                            if url__.is_some() {
                                return Err(serde::de::Error::duplicate_field("url"));
                            }
                            url__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Header => {
                            if header__.is_some() {
                                return Err(serde::de::Error::duplicate_field("header"));
                            }
                            header__ = Some(
                                map_.next_value::<std::collections::HashMap<_, _>>()?
                            );
                        }
                    }
                }
                Ok(callback::Nexus {
                    url: url__.unwrap_or_default(),
                    header: header__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Callback.Nexus", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for callback::NexusHandler {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.task_queue_name.is_empty() {
            len += 1;
        }
        if !self.service.is_empty() {
            len += 1;
        }
        if !self.operation.is_empty() {
            len += 1;
        }
        if self.source_context.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Callback.NexusHandler", len)?;
        if !self.task_queue_name.is_empty() {
            struct_ser.serialize_field("taskQueueName", &self.task_queue_name)?;
        }
        if !self.service.is_empty() {
            struct_ser.serialize_field("service", &self.service)?;
        }
        if !self.operation.is_empty() {
            struct_ser.serialize_field("operation", &self.operation)?;
        }
        if let Some(v) = self.source_context.as_ref() {
            struct_ser.serialize_field("sourceContext", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for callback::NexusHandler {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "task_queue_name",
            "taskQueueName",
            "service",
            "operation",
            "source_context",
            "sourceContext",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TaskQueueName,
            Service,
            Operation,
            SourceContext,
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
                            "taskQueueName" | "task_queue_name" => Ok(GeneratedField::TaskQueueName),
                            "service" => Ok(GeneratedField::Service),
                            "operation" => Ok(GeneratedField::Operation),
                            "sourceContext" | "source_context" => Ok(GeneratedField::SourceContext),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = callback::NexusHandler;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Callback.NexusHandler")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<callback::NexusHandler, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut task_queue_name__ = None;
                let mut service__ = None;
                let mut operation__ = None;
                let mut source_context__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::TaskQueueName => {
                            if task_queue_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("taskQueueName"));
                            }
                            task_queue_name__ = Some(map_.next_value()?);
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
                        GeneratedField::SourceContext => {
                            if source_context__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sourceContext"));
                            }
                            source_context__ = map_.next_value()?;
                        }
                    }
                }
                Ok(callback::NexusHandler {
                    task_queue_name: task_queue_name__.unwrap_or_default(),
                    service: service__.unwrap_or_default(),
                    operation: operation__.unwrap_or_default(),
                    source_context: source_context__,
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Callback.NexusHandler", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DataBlob {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.encoding_type != 0 {
            len += 1;
        }
        if !self.data.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.DataBlob", len)?;
        if self.encoding_type != 0 {
            let v = super::super::enums::v1::EncodingType::try_from(self.encoding_type)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.encoding_type)))?;
            struct_ser.serialize_field("encodingType", &v)?;
        }
        if !self.data.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("data", pbjson::private::base64::encode(&self.data).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DataBlob {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "encoding_type",
            "encodingType",
            "data",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            EncodingType,
            Data,
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
                            "encodingType" | "encoding_type" => Ok(GeneratedField::EncodingType),
                            "data" => Ok(GeneratedField::Data),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DataBlob;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.DataBlob")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<DataBlob, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut encoding_type__ = None;
                let mut data__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::EncodingType => {
                            if encoding_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("encodingType"));
                            }
                            encoding_type__ = Some(map_.next_value::<super::super::enums::v1::EncodingType>()? as i32);
                        }
                        GeneratedField::Data => {
                            if data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("data"));
                            }
                            data__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(DataBlob {
                    encoding_type: encoding_type__.unwrap_or_default(),
                    data: data__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.DataBlob", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Execution {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.r#type != 0 {
            len += 1;
        }
        if !self.business_id.is_empty() {
            len += 1;
        }
        if !self.run_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Execution", len)?;
        if self.r#type != 0 {
            let v = super::super::enums::v1::ExecutionType::try_from(self.r#type)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.r#type)))?;
            struct_ser.serialize_field("type", &v)?;
        }
        if !self.business_id.is_empty() {
            struct_ser.serialize_field("businessId", &self.business_id)?;
        }
        if !self.run_id.is_empty() {
            struct_ser.serialize_field("runId", &self.run_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Execution {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "type",
            "business_id",
            "businessId",
            "run_id",
            "runId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Type,
            BusinessId,
            RunId,
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
                            "businessId" | "business_id" => Ok(GeneratedField::BusinessId),
                            "runId" | "run_id" => Ok(GeneratedField::RunId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Execution;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Execution")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Execution, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut r#type__ = None;
                let mut business_id__ = None;
                let mut run_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Type => {
                            if r#type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("type"));
                            }
                            r#type__ = Some(map_.next_value::<super::super::enums::v1::ExecutionType>()? as i32);
                        }
                        GeneratedField::BusinessId => {
                            if business_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("businessId"));
                            }
                            business_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RunId => {
                            if run_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("runId"));
                            }
                            run_id__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(Execution {
                    r#type: r#type__.unwrap_or_default(),
                    business_id: business_id__.unwrap_or_default(),
                    run_id: run_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Execution", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for FastForwardConfig {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.id.is_empty() {
            len += 1;
        }
        if self.duration.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.FastForwardConfig", len)?;
        if !self.id.is_empty() {
            struct_ser.serialize_field("id", &self.id)?;
        }
        if let Some(v) = self.duration.as_ref() {
            struct_ser.serialize_field("duration", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for FastForwardConfig {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "id",
            "duration",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            Duration,
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
                            "id" => Ok(GeneratedField::Id),
                            "duration" => Ok(GeneratedField::Duration),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = FastForwardConfig;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.FastForwardConfig")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<FastForwardConfig, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                let mut duration__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Duration => {
                            if duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("duration"));
                            }
                            duration__ = map_.next_value()?;
                        }
                    }
                }
                Ok(FastForwardConfig {
                    id: id__.unwrap_or_default(),
                    duration: duration__,
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.FastForwardConfig", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Header {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.fields.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Header", len)?;
        if !self.fields.is_empty() {
            struct_ser.serialize_field("fields", &self.fields)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Header {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "fields",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Fields,
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
                            "fields" => Ok(GeneratedField::Fields),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Header;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Header")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Header, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut fields__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Fields => {
                            if fields__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fields"));
                            }
                            fields__ = Some(
                                map_.next_value::<std::collections::HashMap<_, _>>()?
                            );
                        }
                    }
                }
                Ok(Header {
                    fields: fields__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Header", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Link {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.variant.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Link", len)?;
        if let Some(v) = self.variant.as_ref() {
            match v {
                link::Variant::WorkflowEvent(v) => {
                    struct_ser.serialize_field("workflowEvent", v)?;
                }
                link::Variant::BatchJob(v) => {
                    struct_ser.serialize_field("batchJob", v)?;
                }
                link::Variant::Activity(v) => {
                    struct_ser.serialize_field("activity", v)?;
                }
                link::Variant::NexusOperation(v) => {
                    struct_ser.serialize_field("nexusOperation", v)?;
                }
                link::Variant::Workflow(v) => {
                    struct_ser.serialize_field("workflow", v)?;
                }
                link::Variant::Callback(v) => {
                    struct_ser.serialize_field("callback", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Link {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "workflow_event",
            "workflowEvent",
            "batch_job",
            "batchJob",
            "activity",
            "nexus_operation",
            "nexusOperation",
            "workflow",
            "callback",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            WorkflowEvent,
            BatchJob,
            Activity,
            NexusOperation,
            Workflow,
            Callback,
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
                            "workflowEvent" | "workflow_event" => Ok(GeneratedField::WorkflowEvent),
                            "batchJob" | "batch_job" => Ok(GeneratedField::BatchJob),
                            "activity" => Ok(GeneratedField::Activity),
                            "nexusOperation" | "nexus_operation" => Ok(GeneratedField::NexusOperation),
                            "workflow" => Ok(GeneratedField::Workflow),
                            "callback" => Ok(GeneratedField::Callback),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Link;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Link")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Link, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut variant__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::WorkflowEvent => {
                            if variant__.is_some() {
                                return Err(serde::de::Error::duplicate_field("workflowEvent"));
                            }
                            variant__ = map_.next_value::<::std::option::Option<_>>()?.map(link::Variant::WorkflowEvent)
;
                        }
                        GeneratedField::BatchJob => {
                            if variant__.is_some() {
                                return Err(serde::de::Error::duplicate_field("batchJob"));
                            }
                            variant__ = map_.next_value::<::std::option::Option<_>>()?.map(link::Variant::BatchJob)
;
                        }
                        GeneratedField::Activity => {
                            if variant__.is_some() {
                                return Err(serde::de::Error::duplicate_field("activity"));
                            }
                            variant__ = map_.next_value::<::std::option::Option<_>>()?.map(link::Variant::Activity)
;
                        }
                        GeneratedField::NexusOperation => {
                            if variant__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nexusOperation"));
                            }
                            variant__ = map_.next_value::<::std::option::Option<_>>()?.map(link::Variant::NexusOperation)
;
                        }
                        GeneratedField::Workflow => {
                            if variant__.is_some() {
                                return Err(serde::de::Error::duplicate_field("workflow"));
                            }
                            variant__ = map_.next_value::<::std::option::Option<_>>()?.map(link::Variant::Workflow)
;
                        }
                        GeneratedField::Callback => {
                            if variant__.is_some() {
                                return Err(serde::de::Error::duplicate_field("callback"));
                            }
                            variant__ = map_.next_value::<::std::option::Option<_>>()?.map(link::Variant::Callback)
;
                        }
                    }
                }
                Ok(Link {
                    variant: variant__,
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Link", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for link::Activity {
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
        if !self.activity_id.is_empty() {
            len += 1;
        }
        if !self.run_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Link.Activity", len)?;
        if !self.namespace.is_empty() {
            struct_ser.serialize_field("namespace", &self.namespace)?;
        }
        if !self.activity_id.is_empty() {
            struct_ser.serialize_field("activityId", &self.activity_id)?;
        }
        if !self.run_id.is_empty() {
            struct_ser.serialize_field("runId", &self.run_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for link::Activity {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "namespace",
            "activity_id",
            "activityId",
            "run_id",
            "runId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Namespace,
            ActivityId,
            RunId,
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
                            "activityId" | "activity_id" => Ok(GeneratedField::ActivityId),
                            "runId" | "run_id" => Ok(GeneratedField::RunId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = link::Activity;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Link.Activity")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<link::Activity, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut namespace__ = None;
                let mut activity_id__ = None;
                let mut run_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Namespace => {
                            if namespace__.is_some() {
                                return Err(serde::de::Error::duplicate_field("namespace"));
                            }
                            namespace__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ActivityId => {
                            if activity_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("activityId"));
                            }
                            activity_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RunId => {
                            if run_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("runId"));
                            }
                            run_id__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(link::Activity {
                    namespace: namespace__.unwrap_or_default(),
                    activity_id: activity_id__.unwrap_or_default(),
                    run_id: run_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Link.Activity", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for link::BatchJob {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.job_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Link.BatchJob", len)?;
        if !self.job_id.is_empty() {
            struct_ser.serialize_field("jobId", &self.job_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for link::BatchJob {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "job_id",
            "jobId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            JobId,
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
                            "jobId" | "job_id" => Ok(GeneratedField::JobId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = link::BatchJob;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Link.BatchJob")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<link::BatchJob, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut job_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::JobId => {
                            if job_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("jobId"));
                            }
                            job_id__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(link::BatchJob {
                    job_id: job_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Link.BatchJob", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for link::Callback {
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
        if self.execution.is_some() {
            len += 1;
        }
        if !self.component_path.is_empty() {
            len += 1;
        }
        if !self.request_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Link.Callback", len)?;
        if !self.namespace.is_empty() {
            struct_ser.serialize_field("namespace", &self.namespace)?;
        }
        if let Some(v) = self.execution.as_ref() {
            struct_ser.serialize_field("execution", v)?;
        }
        if !self.component_path.is_empty() {
            struct_ser.serialize_field("componentPath", &self.component_path)?;
        }
        if !self.request_id.is_empty() {
            struct_ser.serialize_field("requestId", &self.request_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for link::Callback {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "namespace",
            "execution",
            "component_path",
            "componentPath",
            "request_id",
            "requestId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Namespace,
            Execution,
            ComponentPath,
            RequestId,
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
                            "execution" => Ok(GeneratedField::Execution),
                            "componentPath" | "component_path" => Ok(GeneratedField::ComponentPath),
                            "requestId" | "request_id" => Ok(GeneratedField::RequestId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = link::Callback;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Link.Callback")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<link::Callback, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut namespace__ = None;
                let mut execution__ = None;
                let mut component_path__ = None;
                let mut request_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Namespace => {
                            if namespace__.is_some() {
                                return Err(serde::de::Error::duplicate_field("namespace"));
                            }
                            namespace__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Execution => {
                            if execution__.is_some() {
                                return Err(serde::de::Error::duplicate_field("execution"));
                            }
                            execution__ = map_.next_value()?;
                        }
                        GeneratedField::ComponentPath => {
                            if component_path__.is_some() {
                                return Err(serde::de::Error::duplicate_field("componentPath"));
                            }
                            component_path__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RequestId => {
                            if request_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requestId"));
                            }
                            request_id__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(link::Callback {
                    namespace: namespace__.unwrap_or_default(),
                    execution: execution__,
                    component_path: component_path__.unwrap_or_default(),
                    request_id: request_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Link.Callback", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for link::NexusOperation {
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
        if !self.operation_id.is_empty() {
            len += 1;
        }
        if !self.run_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Link.NexusOperation", len)?;
        if !self.namespace.is_empty() {
            struct_ser.serialize_field("namespace", &self.namespace)?;
        }
        if !self.operation_id.is_empty() {
            struct_ser.serialize_field("operationId", &self.operation_id)?;
        }
        if !self.run_id.is_empty() {
            struct_ser.serialize_field("runId", &self.run_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for link::NexusOperation {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "namespace",
            "operation_id",
            "operationId",
            "run_id",
            "runId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Namespace,
            OperationId,
            RunId,
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
                            "operationId" | "operation_id" => Ok(GeneratedField::OperationId),
                            "runId" | "run_id" => Ok(GeneratedField::RunId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = link::NexusOperation;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Link.NexusOperation")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<link::NexusOperation, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut namespace__ = None;
                let mut operation_id__ = None;
                let mut run_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Namespace => {
                            if namespace__.is_some() {
                                return Err(serde::de::Error::duplicate_field("namespace"));
                            }
                            namespace__ = Some(map_.next_value()?);
                        }
                        GeneratedField::OperationId => {
                            if operation_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("operationId"));
                            }
                            operation_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RunId => {
                            if run_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("runId"));
                            }
                            run_id__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(link::NexusOperation {
                    namespace: namespace__.unwrap_or_default(),
                    operation_id: operation_id__.unwrap_or_default(),
                    run_id: run_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Link.NexusOperation", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for link::Workflow {
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
        if !self.workflow_id.is_empty() {
            len += 1;
        }
        if !self.run_id.is_empty() {
            len += 1;
        }
        if !self.reason.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Link.Workflow", len)?;
        if !self.namespace.is_empty() {
            struct_ser.serialize_field("namespace", &self.namespace)?;
        }
        if !self.workflow_id.is_empty() {
            struct_ser.serialize_field("workflowId", &self.workflow_id)?;
        }
        if !self.run_id.is_empty() {
            struct_ser.serialize_field("runId", &self.run_id)?;
        }
        if !self.reason.is_empty() {
            struct_ser.serialize_field("reason", &self.reason)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for link::Workflow {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "namespace",
            "workflow_id",
            "workflowId",
            "run_id",
            "runId",
            "reason",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Namespace,
            WorkflowId,
            RunId,
            Reason,
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
                            "workflowId" | "workflow_id" => Ok(GeneratedField::WorkflowId),
                            "runId" | "run_id" => Ok(GeneratedField::RunId),
                            "reason" => Ok(GeneratedField::Reason),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = link::Workflow;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Link.Workflow")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<link::Workflow, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut namespace__ = None;
                let mut workflow_id__ = None;
                let mut run_id__ = None;
                let mut reason__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Namespace => {
                            if namespace__.is_some() {
                                return Err(serde::de::Error::duplicate_field("namespace"));
                            }
                            namespace__ = Some(map_.next_value()?);
                        }
                        GeneratedField::WorkflowId => {
                            if workflow_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("workflowId"));
                            }
                            workflow_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RunId => {
                            if run_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("runId"));
                            }
                            run_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Reason => {
                            if reason__.is_some() {
                                return Err(serde::de::Error::duplicate_field("reason"));
                            }
                            reason__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(link::Workflow {
                    namespace: namespace__.unwrap_or_default(),
                    workflow_id: workflow_id__.unwrap_or_default(),
                    run_id: run_id__.unwrap_or_default(),
                    reason: reason__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Link.Workflow", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for link::WorkflowEvent {
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
        if !self.workflow_id.is_empty() {
            len += 1;
        }
        if !self.run_id.is_empty() {
            len += 1;
        }
        if self.reference.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Link.WorkflowEvent", len)?;
        if !self.namespace.is_empty() {
            struct_ser.serialize_field("namespace", &self.namespace)?;
        }
        if !self.workflow_id.is_empty() {
            struct_ser.serialize_field("workflowId", &self.workflow_id)?;
        }
        if !self.run_id.is_empty() {
            struct_ser.serialize_field("runId", &self.run_id)?;
        }
        if let Some(v) = self.reference.as_ref() {
            match v {
                link::workflow_event::Reference::EventRef(v) => {
                    struct_ser.serialize_field("eventRef", v)?;
                }
                link::workflow_event::Reference::RequestIdRef(v) => {
                    struct_ser.serialize_field("requestIdRef", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for link::WorkflowEvent {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "namespace",
            "workflow_id",
            "workflowId",
            "run_id",
            "runId",
            "event_ref",
            "eventRef",
            "request_id_ref",
            "requestIdRef",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Namespace,
            WorkflowId,
            RunId,
            EventRef,
            RequestIdRef,
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
                            "workflowId" | "workflow_id" => Ok(GeneratedField::WorkflowId),
                            "runId" | "run_id" => Ok(GeneratedField::RunId),
                            "eventRef" | "event_ref" => Ok(GeneratedField::EventRef),
                            "requestIdRef" | "request_id_ref" => Ok(GeneratedField::RequestIdRef),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = link::WorkflowEvent;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Link.WorkflowEvent")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<link::WorkflowEvent, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut namespace__ = None;
                let mut workflow_id__ = None;
                let mut run_id__ = None;
                let mut reference__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Namespace => {
                            if namespace__.is_some() {
                                return Err(serde::de::Error::duplicate_field("namespace"));
                            }
                            namespace__ = Some(map_.next_value()?);
                        }
                        GeneratedField::WorkflowId => {
                            if workflow_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("workflowId"));
                            }
                            workflow_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RunId => {
                            if run_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("runId"));
                            }
                            run_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::EventRef => {
                            if reference__.is_some() {
                                return Err(serde::de::Error::duplicate_field("eventRef"));
                            }
                            reference__ = map_.next_value::<::std::option::Option<_>>()?.map(link::workflow_event::Reference::EventRef)
;
                        }
                        GeneratedField::RequestIdRef => {
                            if reference__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requestIdRef"));
                            }
                            reference__ = map_.next_value::<::std::option::Option<_>>()?.map(link::workflow_event::Reference::RequestIdRef)
;
                        }
                    }
                }
                Ok(link::WorkflowEvent {
                    namespace: namespace__.unwrap_or_default(),
                    workflow_id: workflow_id__.unwrap_or_default(),
                    run_id: run_id__.unwrap_or_default(),
                    reference: reference__,
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Link.WorkflowEvent", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for link::workflow_event::EventReference {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.event_id != 0 {
            len += 1;
        }
        if self.event_type != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Link.WorkflowEvent.EventReference", len)?;
        if self.event_id != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("eventId", ToString::to_string(&self.event_id).as_str())?;
        }
        if self.event_type != 0 {
            let v = super::super::enums::v1::EventType::try_from(self.event_type)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.event_type)))?;
            struct_ser.serialize_field("eventType", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for link::workflow_event::EventReference {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "event_id",
            "eventId",
            "event_type",
            "eventType",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            EventId,
            EventType,
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
                            "eventId" | "event_id" => Ok(GeneratedField::EventId),
                            "eventType" | "event_type" => Ok(GeneratedField::EventType),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = link::workflow_event::EventReference;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Link.WorkflowEvent.EventReference")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<link::workflow_event::EventReference, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut event_id__ = None;
                let mut event_type__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::EventId => {
                            if event_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("eventId"));
                            }
                            event_id__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::EventType => {
                            if event_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("eventType"));
                            }
                            event_type__ = Some(map_.next_value::<super::super::enums::v1::EventType>()? as i32);
                        }
                    }
                }
                Ok(link::workflow_event::EventReference {
                    event_id: event_id__.unwrap_or_default(),
                    event_type: event_type__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Link.WorkflowEvent.EventReference", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for link::workflow_event::RequestIdReference {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.request_id.is_empty() {
            len += 1;
        }
        if self.event_type != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Link.WorkflowEvent.RequestIdReference", len)?;
        if !self.request_id.is_empty() {
            struct_ser.serialize_field("requestId", &self.request_id)?;
        }
        if self.event_type != 0 {
            let v = super::super::enums::v1::EventType::try_from(self.event_type)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.event_type)))?;
            struct_ser.serialize_field("eventType", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for link::workflow_event::RequestIdReference {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "request_id",
            "requestId",
            "event_type",
            "eventType",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RequestId,
            EventType,
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
                            "requestId" | "request_id" => Ok(GeneratedField::RequestId),
                            "eventType" | "event_type" => Ok(GeneratedField::EventType),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = link::workflow_event::RequestIdReference;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Link.WorkflowEvent.RequestIdReference")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<link::workflow_event::RequestIdReference, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut request_id__ = None;
                let mut event_type__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::RequestId => {
                            if request_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requestId"));
                            }
                            request_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::EventType => {
                            if event_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("eventType"));
                            }
                            event_type__ = Some(map_.next_value::<super::super::enums::v1::EventType>()? as i32);
                        }
                    }
                }
                Ok(link::workflow_event::RequestIdReference {
                    request_id: request_id__.unwrap_or_default(),
                    event_type: event_type__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Link.WorkflowEvent.RequestIdReference", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Memo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.fields.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Memo", len)?;
        if !self.fields.is_empty() {
            struct_ser.serialize_field("fields", &self.fields)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Memo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "fields",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Fields,
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
                            "fields" => Ok(GeneratedField::Fields),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Memo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Memo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Memo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut fields__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Fields => {
                            if fields__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fields"));
                            }
                            fields__ = Some(
                                map_.next_value::<std::collections::HashMap<_, _>>()?
                            );
                        }
                    }
                }
                Ok(Memo {
                    fields: fields__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Memo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MeteringMetadata {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.nonfirst_local_activity_execution_attempts != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.MeteringMetadata", len)?;
        if self.nonfirst_local_activity_execution_attempts != 0 {
            struct_ser.serialize_field("nonfirstLocalActivityExecutionAttempts", &self.nonfirst_local_activity_execution_attempts)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MeteringMetadata {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "nonfirst_local_activity_execution_attempts",
            "nonfirstLocalActivityExecutionAttempts",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NonfirstLocalActivityExecutionAttempts,
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
                            "nonfirstLocalActivityExecutionAttempts" | "nonfirst_local_activity_execution_attempts" => Ok(GeneratedField::NonfirstLocalActivityExecutionAttempts),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MeteringMetadata;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.MeteringMetadata")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MeteringMetadata, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut nonfirst_local_activity_execution_attempts__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NonfirstLocalActivityExecutionAttempts => {
                            if nonfirst_local_activity_execution_attempts__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nonfirstLocalActivityExecutionAttempts"));
                            }
                            nonfirst_local_activity_execution_attempts__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(MeteringMetadata {
                    nonfirst_local_activity_execution_attempts: nonfirst_local_activity_execution_attempts__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.MeteringMetadata", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for OnConflictOptions {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.attach_request_id {
            len += 1;
        }
        if self.attach_completion_callbacks {
            len += 1;
        }
        if self.attach_links {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.OnConflictOptions", len)?;
        if self.attach_request_id {
            struct_ser.serialize_field("attachRequestId", &self.attach_request_id)?;
        }
        if self.attach_completion_callbacks {
            struct_ser.serialize_field("attachCompletionCallbacks", &self.attach_completion_callbacks)?;
        }
        if self.attach_links {
            struct_ser.serialize_field("attachLinks", &self.attach_links)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for OnConflictOptions {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "attach_request_id",
            "attachRequestId",
            "attach_completion_callbacks",
            "attachCompletionCallbacks",
            "attach_links",
            "attachLinks",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AttachRequestId,
            AttachCompletionCallbacks,
            AttachLinks,
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
                            "attachRequestId" | "attach_request_id" => Ok(GeneratedField::AttachRequestId),
                            "attachCompletionCallbacks" | "attach_completion_callbacks" => Ok(GeneratedField::AttachCompletionCallbacks),
                            "attachLinks" | "attach_links" => Ok(GeneratedField::AttachLinks),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = OnConflictOptions;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.OnConflictOptions")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<OnConflictOptions, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut attach_request_id__ = None;
                let mut attach_completion_callbacks__ = None;
                let mut attach_links__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AttachRequestId => {
                            if attach_request_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("attachRequestId"));
                            }
                            attach_request_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::AttachCompletionCallbacks => {
                            if attach_completion_callbacks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("attachCompletionCallbacks"));
                            }
                            attach_completion_callbacks__ = Some(map_.next_value()?);
                        }
                        GeneratedField::AttachLinks => {
                            if attach_links__.is_some() {
                                return Err(serde::de::Error::duplicate_field("attachLinks"));
                            }
                            attach_links__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(OnConflictOptions {
                    attach_request_id: attach_request_id__.unwrap_or_default(),
                    attach_completion_callbacks: attach_completion_callbacks__.unwrap_or_default(),
                    attach_links: attach_links__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.OnConflictOptions", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Payload {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.metadata.is_empty() {
            len += 1;
        }
        if !self.data.is_empty() {
            len += 1;
        }
        if !self.external_payloads.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Payload", len)?;
        if !self.metadata.is_empty() {
            let v: std::collections::HashMap<_, _> = self.metadata.iter()
                .map(|(k, v)| (k, pbjson::private::base64::encode(v))).collect();
            struct_ser.serialize_field("metadata", &v)?;
        }
        if !self.data.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("data", pbjson::private::base64::encode(&self.data).as_str())?;
        }
        if !self.external_payloads.is_empty() {
            struct_ser.serialize_field("externalPayloads", &self.external_payloads)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Payload {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "metadata",
            "data",
            "external_payloads",
            "externalPayloads",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Metadata,
            Data,
            ExternalPayloads,
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
                            "metadata" => Ok(GeneratedField::Metadata),
                            "data" => Ok(GeneratedField::Data),
                            "externalPayloads" | "external_payloads" => Ok(GeneratedField::ExternalPayloads),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Payload;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Payload")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Payload, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut metadata__ = None;
                let mut data__ = None;
                let mut external_payloads__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Metadata => {
                            if metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("metadata"));
                            }
                            metadata__ = Some(
                                map_.next_value::<std::collections::HashMap<_, ::pbjson::private::BytesDeserialize<_>>>()?
                                    .into_iter().map(|(k,v)| (k, v.0)).collect()
                            );
                        }
                        GeneratedField::Data => {
                            if data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("data"));
                            }
                            data__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ExternalPayloads => {
                            if external_payloads__.is_some() {
                                return Err(serde::de::Error::duplicate_field("externalPayloads"));
                            }
                            external_payloads__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(Payload {
                    metadata: metadata__.unwrap_or_default(),
                    data: data__.unwrap_or_default(),
                    external_payloads: external_payloads__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Payload", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for payload::ExternalPayloadDetails {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.size_bytes != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Payload.ExternalPayloadDetails", len)?;
        if self.size_bytes != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("sizeBytes", ToString::to_string(&self.size_bytes).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for payload::ExternalPayloadDetails {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "size_bytes",
            "sizeBytes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            SizeBytes,
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
                            "sizeBytes" | "size_bytes" => Ok(GeneratedField::SizeBytes),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = payload::ExternalPayloadDetails;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Payload.ExternalPayloadDetails")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<payload::ExternalPayloadDetails, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut size_bytes__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::SizeBytes => {
                            if size_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sizeBytes"));
                            }
                            size_bytes__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(payload::ExternalPayloadDetails {
                    size_bytes: size_bytes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Payload.ExternalPayloadDetails", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Payloads {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.payloads.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Payloads", len)?;
        if !self.payloads.is_empty() {
            struct_ser.serialize_field("payloads", &self.payloads)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Payloads {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "payloads",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Payloads,
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
                            "payloads" => Ok(GeneratedField::Payloads),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Payloads;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Payloads")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Payloads, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut payloads__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Payloads => {
                            if payloads__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payloads"));
                            }
                            payloads__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(Payloads {
                    payloads: payloads__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Payloads", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Principal {
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
        if !self.name.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Principal", len)?;
        if !self.r#type.is_empty() {
            struct_ser.serialize_field("type", &self.r#type)?;
        }
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Principal {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "type",
            "name",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Type,
            Name,
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
                            "name" => Ok(GeneratedField::Name),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Principal;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Principal")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Principal, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut r#type__ = None;
                let mut name__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Type => {
                            if r#type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("type"));
                            }
                            r#type__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(Principal {
                    r#type: r#type__.unwrap_or_default(),
                    name: name__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Principal", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Priority {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.priority_key != 0 {
            len += 1;
        }
        if !self.fairness_key.is_empty() {
            len += 1;
        }
        if self.fairness_weight != 0. {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.Priority", len)?;
        if self.priority_key != 0 {
            struct_ser.serialize_field("priorityKey", &self.priority_key)?;
        }
        if !self.fairness_key.is_empty() {
            struct_ser.serialize_field("fairnessKey", &self.fairness_key)?;
        }
        if self.fairness_weight != 0. {
            struct_ser.serialize_field("fairnessWeight", &self.fairness_weight)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Priority {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "priority_key",
            "priorityKey",
            "fairness_key",
            "fairnessKey",
            "fairness_weight",
            "fairnessWeight",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            PriorityKey,
            FairnessKey,
            FairnessWeight,
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
                            "priorityKey" | "priority_key" => Ok(GeneratedField::PriorityKey),
                            "fairnessKey" | "fairness_key" => Ok(GeneratedField::FairnessKey),
                            "fairnessWeight" | "fairness_weight" => Ok(GeneratedField::FairnessWeight),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Priority;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.Priority")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Priority, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut priority_key__ = None;
                let mut fairness_key__ = None;
                let mut fairness_weight__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::PriorityKey => {
                            if priority_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("priorityKey"));
                            }
                            priority_key__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::FairnessKey => {
                            if fairness_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fairnessKey"));
                            }
                            fairness_key__ = Some(map_.next_value()?);
                        }
                        GeneratedField::FairnessWeight => {
                            if fairness_weight__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fairnessWeight"));
                            }
                            fairness_weight__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(Priority {
                    priority_key: priority_key__.unwrap_or_default(),
                    fairness_key: fairness_key__.unwrap_or_default(),
                    fairness_weight: fairness_weight__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.Priority", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ResetOptions {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.reset_reapply_type != 0 {
            len += 1;
        }
        if self.current_run_only {
            len += 1;
        }
        if !self.reset_reapply_exclude_types.is_empty() {
            len += 1;
        }
        if self.target.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.ResetOptions", len)?;
        if self.reset_reapply_type != 0 {
            let v = super::super::enums::v1::ResetReapplyType::try_from(self.reset_reapply_type)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.reset_reapply_type)))?;
            struct_ser.serialize_field("resetReapplyType", &v)?;
        }
        if self.current_run_only {
            struct_ser.serialize_field("currentRunOnly", &self.current_run_only)?;
        }
        if !self.reset_reapply_exclude_types.is_empty() {
            let v = self.reset_reapply_exclude_types.iter().cloned().map(|v| {
                super::super::enums::v1::ResetReapplyExcludeType::try_from(v)
                    .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", v)))
                }).collect::<std::result::Result<Vec<_>, _>>()?;
            struct_ser.serialize_field("resetReapplyExcludeTypes", &v)?;
        }
        if let Some(v) = self.target.as_ref() {
            match v {
                reset_options::Target::FirstWorkflowTask(v) => {
                    struct_ser.serialize_field("firstWorkflowTask", v)?;
                }
                reset_options::Target::LastWorkflowTask(v) => {
                    struct_ser.serialize_field("lastWorkflowTask", v)?;
                }
                reset_options::Target::WorkflowTaskId(v) => {
                    #[allow(clippy::needless_borrow)]
                    #[allow(clippy::needless_borrows_for_generic_args)]
                    struct_ser.serialize_field("workflowTaskId", ToString::to_string(&v).as_str())?;
                }
                reset_options::Target::BuildId(v) => {
                    struct_ser.serialize_field("buildId", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ResetOptions {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "reset_reapply_type",
            "resetReapplyType",
            "current_run_only",
            "currentRunOnly",
            "reset_reapply_exclude_types",
            "resetReapplyExcludeTypes",
            "first_workflow_task",
            "firstWorkflowTask",
            "last_workflow_task",
            "lastWorkflowTask",
            "workflow_task_id",
            "workflowTaskId",
            "build_id",
            "buildId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ResetReapplyType,
            CurrentRunOnly,
            ResetReapplyExcludeTypes,
            FirstWorkflowTask,
            LastWorkflowTask,
            WorkflowTaskId,
            BuildId,
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
                            "resetReapplyType" | "reset_reapply_type" => Ok(GeneratedField::ResetReapplyType),
                            "currentRunOnly" | "current_run_only" => Ok(GeneratedField::CurrentRunOnly),
                            "resetReapplyExcludeTypes" | "reset_reapply_exclude_types" => Ok(GeneratedField::ResetReapplyExcludeTypes),
                            "firstWorkflowTask" | "first_workflow_task" => Ok(GeneratedField::FirstWorkflowTask),
                            "lastWorkflowTask" | "last_workflow_task" => Ok(GeneratedField::LastWorkflowTask),
                            "workflowTaskId" | "workflow_task_id" => Ok(GeneratedField::WorkflowTaskId),
                            "buildId" | "build_id" => Ok(GeneratedField::BuildId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ResetOptions;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.ResetOptions")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ResetOptions, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut reset_reapply_type__ = None;
                let mut current_run_only__ = None;
                let mut reset_reapply_exclude_types__ = None;
                let mut target__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ResetReapplyType => {
                            if reset_reapply_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("resetReapplyType"));
                            }
                            reset_reapply_type__ = Some(map_.next_value::<super::super::enums::v1::ResetReapplyType>()? as i32);
                        }
                        GeneratedField::CurrentRunOnly => {
                            if current_run_only__.is_some() {
                                return Err(serde::de::Error::duplicate_field("currentRunOnly"));
                            }
                            current_run_only__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ResetReapplyExcludeTypes => {
                            if reset_reapply_exclude_types__.is_some() {
                                return Err(serde::de::Error::duplicate_field("resetReapplyExcludeTypes"));
                            }
                            reset_reapply_exclude_types__ = Some(map_.next_value::<Vec<super::super::enums::v1::ResetReapplyExcludeType>>()?.into_iter().map(|x| x as i32).collect());
                        }
                        GeneratedField::FirstWorkflowTask => {
                            if target__.is_some() {
                                return Err(serde::de::Error::duplicate_field("firstWorkflowTask"));
                            }
                            target__ = map_.next_value::<::std::option::Option<_>>()?.map(reset_options::Target::FirstWorkflowTask)
;
                        }
                        GeneratedField::LastWorkflowTask => {
                            if target__.is_some() {
                                return Err(serde::de::Error::duplicate_field("lastWorkflowTask"));
                            }
                            target__ = map_.next_value::<::std::option::Option<_>>()?.map(reset_options::Target::LastWorkflowTask)
;
                        }
                        GeneratedField::WorkflowTaskId => {
                            if target__.is_some() {
                                return Err(serde::de::Error::duplicate_field("workflowTaskId"));
                            }
                            target__ = map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| reset_options::Target::WorkflowTaskId(x.0));
                        }
                        GeneratedField::BuildId => {
                            if target__.is_some() {
                                return Err(serde::de::Error::duplicate_field("buildId"));
                            }
                            target__ = map_.next_value::<::std::option::Option<_>>()?.map(reset_options::Target::BuildId);
                        }
                    }
                }
                Ok(ResetOptions {
                    reset_reapply_type: reset_reapply_type__.unwrap_or_default(),
                    current_run_only: current_run_only__.unwrap_or_default(),
                    reset_reapply_exclude_types: reset_reapply_exclude_types__.unwrap_or_default(),
                    target: target__,
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.ResetOptions", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RetryPolicy {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.initial_interval.is_some() {
            len += 1;
        }
        if self.backoff_coefficient != 0. {
            len += 1;
        }
        if self.maximum_interval.is_some() {
            len += 1;
        }
        if self.maximum_attempts != 0 {
            len += 1;
        }
        if !self.non_retryable_error_types.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.RetryPolicy", len)?;
        if let Some(v) = self.initial_interval.as_ref() {
            struct_ser.serialize_field("initialInterval", v)?;
        }
        if self.backoff_coefficient != 0. {
            struct_ser.serialize_field("backoffCoefficient", &self.backoff_coefficient)?;
        }
        if let Some(v) = self.maximum_interval.as_ref() {
            struct_ser.serialize_field("maximumInterval", v)?;
        }
        if self.maximum_attempts != 0 {
            struct_ser.serialize_field("maximumAttempts", &self.maximum_attempts)?;
        }
        if !self.non_retryable_error_types.is_empty() {
            struct_ser.serialize_field("nonRetryableErrorTypes", &self.non_retryable_error_types)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RetryPolicy {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "initial_interval",
            "initialInterval",
            "backoff_coefficient",
            "backoffCoefficient",
            "maximum_interval",
            "maximumInterval",
            "maximum_attempts",
            "maximumAttempts",
            "non_retryable_error_types",
            "nonRetryableErrorTypes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InitialInterval,
            BackoffCoefficient,
            MaximumInterval,
            MaximumAttempts,
            NonRetryableErrorTypes,
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
                            "initialInterval" | "initial_interval" => Ok(GeneratedField::InitialInterval),
                            "backoffCoefficient" | "backoff_coefficient" => Ok(GeneratedField::BackoffCoefficient),
                            "maximumInterval" | "maximum_interval" => Ok(GeneratedField::MaximumInterval),
                            "maximumAttempts" | "maximum_attempts" => Ok(GeneratedField::MaximumAttempts),
                            "nonRetryableErrorTypes" | "non_retryable_error_types" => Ok(GeneratedField::NonRetryableErrorTypes),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RetryPolicy;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.RetryPolicy")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RetryPolicy, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut initial_interval__ = None;
                let mut backoff_coefficient__ = None;
                let mut maximum_interval__ = None;
                let mut maximum_attempts__ = None;
                let mut non_retryable_error_types__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::InitialInterval => {
                            if initial_interval__.is_some() {
                                return Err(serde::de::Error::duplicate_field("initialInterval"));
                            }
                            initial_interval__ = map_.next_value()?;
                        }
                        GeneratedField::BackoffCoefficient => {
                            if backoff_coefficient__.is_some() {
                                return Err(serde::de::Error::duplicate_field("backoffCoefficient"));
                            }
                            backoff_coefficient__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::MaximumInterval => {
                            if maximum_interval__.is_some() {
                                return Err(serde::de::Error::duplicate_field("maximumInterval"));
                            }
                            maximum_interval__ = map_.next_value()?;
                        }
                        GeneratedField::MaximumAttempts => {
                            if maximum_attempts__.is_some() {
                                return Err(serde::de::Error::duplicate_field("maximumAttempts"));
                            }
                            maximum_attempts__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::NonRetryableErrorTypes => {
                            if non_retryable_error_types__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nonRetryableErrorTypes"));
                            }
                            non_retryable_error_types__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(RetryPolicy {
                    initial_interval: initial_interval__,
                    backoff_coefficient: backoff_coefficient__.unwrap_or_default(),
                    maximum_interval: maximum_interval__,
                    maximum_attempts: maximum_attempts__.unwrap_or_default(),
                    non_retryable_error_types: non_retryable_error_types__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.RetryPolicy", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SearchAttributes {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.indexed_fields.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.SearchAttributes", len)?;
        if !self.indexed_fields.is_empty() {
            struct_ser.serialize_field("indexedFields", &self.indexed_fields)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SearchAttributes {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "indexed_fields",
            "indexedFields",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IndexedFields,
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
                            "indexedFields" | "indexed_fields" => Ok(GeneratedField::IndexedFields),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SearchAttributes;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.SearchAttributes")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SearchAttributes, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut indexed_fields__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::IndexedFields => {
                            if indexed_fields__.is_some() {
                                return Err(serde::de::Error::duplicate_field("indexedFields"));
                            }
                            indexed_fields__ = Some(
                                map_.next_value::<std::collections::HashMap<_, _>>()?
                            );
                        }
                    }
                }
                Ok(SearchAttributes {
                    indexed_fields: indexed_fields__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.SearchAttributes", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TimeSkippingConfig {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.enabled {
            len += 1;
        }
        if self.fast_forward_config.is_some() {
            len += 1;
        }
        if self.disable_propagation {
            len += 1;
        }
        if self.max_session_skip_count != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.TimeSkippingConfig", len)?;
        if self.enabled {
            struct_ser.serialize_field("enabled", &self.enabled)?;
        }
        if let Some(v) = self.fast_forward_config.as_ref() {
            struct_ser.serialize_field("fastForwardConfig", v)?;
        }
        if self.disable_propagation {
            struct_ser.serialize_field("disablePropagation", &self.disable_propagation)?;
        }
        if self.max_session_skip_count != 0 {
            struct_ser.serialize_field("maxSessionSkipCount", &self.max_session_skip_count)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TimeSkippingConfig {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "enabled",
            "fast_forward_config",
            "fastForwardConfig",
            "disable_propagation",
            "disablePropagation",
            "max_session_skip_count",
            "maxSessionSkipCount",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Enabled,
            FastForwardConfig,
            DisablePropagation,
            MaxSessionSkipCount,
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
                            "enabled" => Ok(GeneratedField::Enabled),
                            "fastForwardConfig" | "fast_forward_config" => Ok(GeneratedField::FastForwardConfig),
                            "disablePropagation" | "disable_propagation" => Ok(GeneratedField::DisablePropagation),
                            "maxSessionSkipCount" | "max_session_skip_count" => Ok(GeneratedField::MaxSessionSkipCount),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TimeSkippingConfig;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.TimeSkippingConfig")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TimeSkippingConfig, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut enabled__ = None;
                let mut fast_forward_config__ = None;
                let mut disable_propagation__ = None;
                let mut max_session_skip_count__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Enabled => {
                            if enabled__.is_some() {
                                return Err(serde::de::Error::duplicate_field("enabled"));
                            }
                            enabled__ = Some(map_.next_value()?);
                        }
                        GeneratedField::FastForwardConfig => {
                            if fast_forward_config__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fastForwardConfig"));
                            }
                            fast_forward_config__ = map_.next_value()?;
                        }
                        GeneratedField::DisablePropagation => {
                            if disable_propagation__.is_some() {
                                return Err(serde::de::Error::duplicate_field("disablePropagation"));
                            }
                            disable_propagation__ = Some(map_.next_value()?);
                        }
                        GeneratedField::MaxSessionSkipCount => {
                            if max_session_skip_count__.is_some() {
                                return Err(serde::de::Error::duplicate_field("maxSessionSkipCount"));
                            }
                            max_session_skip_count__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(TimeSkippingConfig {
                    enabled: enabled__.unwrap_or_default(),
                    fast_forward_config: fast_forward_config__,
                    disable_propagation: disable_propagation__.unwrap_or_default(),
                    max_session_skip_count: max_session_skip_count__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.TimeSkippingConfig", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TimeSkippingFastForwardInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.fast_forward_duration.is_some() {
            len += 1;
        }
        if !self.fast_forward_id.is_empty() {
            len += 1;
        }
        if self.target_time.is_some() {
            len += 1;
        }
        if self.has_completed {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.TimeSkippingFastForwardInfo", len)?;
        if let Some(v) = self.fast_forward_duration.as_ref() {
            struct_ser.serialize_field("fastForwardDuration", v)?;
        }
        if !self.fast_forward_id.is_empty() {
            struct_ser.serialize_field("fastForwardId", &self.fast_forward_id)?;
        }
        if let Some(v) = self.target_time.as_ref() {
            struct_ser.serialize_field("targetTime", v)?;
        }
        if self.has_completed {
            struct_ser.serialize_field("hasCompleted", &self.has_completed)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TimeSkippingFastForwardInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "fast_forward_duration",
            "fastForwardDuration",
            "fast_forward_id",
            "fastForwardId",
            "target_time",
            "targetTime",
            "has_completed",
            "hasCompleted",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            FastForwardDuration,
            FastForwardId,
            TargetTime,
            HasCompleted,
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
                            "fastForwardDuration" | "fast_forward_duration" => Ok(GeneratedField::FastForwardDuration),
                            "fastForwardId" | "fast_forward_id" => Ok(GeneratedField::FastForwardId),
                            "targetTime" | "target_time" => Ok(GeneratedField::TargetTime),
                            "hasCompleted" | "has_completed" => Ok(GeneratedField::HasCompleted),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TimeSkippingFastForwardInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.TimeSkippingFastForwardInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TimeSkippingFastForwardInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut fast_forward_duration__ = None;
                let mut fast_forward_id__ = None;
                let mut target_time__ = None;
                let mut has_completed__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::FastForwardDuration => {
                            if fast_forward_duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fastForwardDuration"));
                            }
                            fast_forward_duration__ = map_.next_value()?;
                        }
                        GeneratedField::FastForwardId => {
                            if fast_forward_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fastForwardId"));
                            }
                            fast_forward_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::TargetTime => {
                            if target_time__.is_some() {
                                return Err(serde::de::Error::duplicate_field("targetTime"));
                            }
                            target_time__ = map_.next_value()?;
                        }
                        GeneratedField::HasCompleted => {
                            if has_completed__.is_some() {
                                return Err(serde::de::Error::duplicate_field("hasCompleted"));
                            }
                            has_completed__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(TimeSkippingFastForwardInfo {
                    fast_forward_duration: fast_forward_duration__,
                    fast_forward_id: fast_forward_id__.unwrap_or_default(),
                    target_time: target_time__,
                    has_completed: has_completed__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.TimeSkippingFastForwardInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TimeSkippingInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.current_time.is_some() {
            len += 1;
        }
        if self.effective_config.is_some() {
            len += 1;
        }
        if self.fast_forward_info.is_some() {
            len += 1;
        }
        if self.current_session_skip_count != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.TimeSkippingInfo", len)?;
        if let Some(v) = self.current_time.as_ref() {
            struct_ser.serialize_field("currentTime", v)?;
        }
        if let Some(v) = self.effective_config.as_ref() {
            struct_ser.serialize_field("effectiveConfig", v)?;
        }
        if let Some(v) = self.fast_forward_info.as_ref() {
            struct_ser.serialize_field("fastForwardInfo", v)?;
        }
        if self.current_session_skip_count != 0 {
            struct_ser.serialize_field("currentSessionSkipCount", &self.current_session_skip_count)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TimeSkippingInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "current_time",
            "currentTime",
            "effective_config",
            "effectiveConfig",
            "fast_forward_info",
            "fastForwardInfo",
            "current_session_skip_count",
            "currentSessionSkipCount",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CurrentTime,
            EffectiveConfig,
            FastForwardInfo,
            CurrentSessionSkipCount,
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
                            "currentTime" | "current_time" => Ok(GeneratedField::CurrentTime),
                            "effectiveConfig" | "effective_config" => Ok(GeneratedField::EffectiveConfig),
                            "fastForwardInfo" | "fast_forward_info" => Ok(GeneratedField::FastForwardInfo),
                            "currentSessionSkipCount" | "current_session_skip_count" => Ok(GeneratedField::CurrentSessionSkipCount),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TimeSkippingInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.TimeSkippingInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TimeSkippingInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut current_time__ = None;
                let mut effective_config__ = None;
                let mut fast_forward_info__ = None;
                let mut current_session_skip_count__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::CurrentTime => {
                            if current_time__.is_some() {
                                return Err(serde::de::Error::duplicate_field("currentTime"));
                            }
                            current_time__ = map_.next_value()?;
                        }
                        GeneratedField::EffectiveConfig => {
                            if effective_config__.is_some() {
                                return Err(serde::de::Error::duplicate_field("effectiveConfig"));
                            }
                            effective_config__ = map_.next_value()?;
                        }
                        GeneratedField::FastForwardInfo => {
                            if fast_forward_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fastForwardInfo"));
                            }
                            fast_forward_info__ = map_.next_value()?;
                        }
                        GeneratedField::CurrentSessionSkipCount => {
                            if current_session_skip_count__.is_some() {
                                return Err(serde::de::Error::duplicate_field("currentSessionSkipCount"));
                            }
                            current_session_skip_count__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(TimeSkippingInfo {
                    current_time: current_time__,
                    effective_config: effective_config__,
                    fast_forward_info: fast_forward_info__,
                    current_session_skip_count: current_session_skip_count__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.TimeSkippingInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TimeSkippingStatePropagation {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.initial_skipped_duration.is_some() {
            len += 1;
        }
        if self.fast_forward_target_time.is_some() {
            len += 1;
        }
        if self.initial_skip_count != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.TimeSkippingStatePropagation", len)?;
        if let Some(v) = self.initial_skipped_duration.as_ref() {
            struct_ser.serialize_field("initialSkippedDuration", v)?;
        }
        if let Some(v) = self.fast_forward_target_time.as_ref() {
            struct_ser.serialize_field("fastForwardTargetTime", v)?;
        }
        if self.initial_skip_count != 0 {
            struct_ser.serialize_field("initialSkipCount", &self.initial_skip_count)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TimeSkippingStatePropagation {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "initial_skipped_duration",
            "initialSkippedDuration",
            "fast_forward_target_time",
            "fastForwardTargetTime",
            "initial_skip_count",
            "initialSkipCount",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InitialSkippedDuration,
            FastForwardTargetTime,
            InitialSkipCount,
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
                            "initialSkippedDuration" | "initial_skipped_duration" => Ok(GeneratedField::InitialSkippedDuration),
                            "fastForwardTargetTime" | "fast_forward_target_time" => Ok(GeneratedField::FastForwardTargetTime),
                            "initialSkipCount" | "initial_skip_count" => Ok(GeneratedField::InitialSkipCount),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TimeSkippingStatePropagation;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.TimeSkippingStatePropagation")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TimeSkippingStatePropagation, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut initial_skipped_duration__ = None;
                let mut fast_forward_target_time__ = None;
                let mut initial_skip_count__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::InitialSkippedDuration => {
                            if initial_skipped_duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("initialSkippedDuration"));
                            }
                            initial_skipped_duration__ = map_.next_value()?;
                        }
                        GeneratedField::FastForwardTargetTime => {
                            if fast_forward_target_time__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fastForwardTargetTime"));
                            }
                            fast_forward_target_time__ = map_.next_value()?;
                        }
                        GeneratedField::InitialSkipCount => {
                            if initial_skip_count__.is_some() {
                                return Err(serde::de::Error::duplicate_field("initialSkipCount"));
                            }
                            initial_skip_count__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(TimeSkippingStatePropagation {
                    initial_skipped_duration: initial_skipped_duration__,
                    fast_forward_target_time: fast_forward_target_time__,
                    initial_skip_count: initial_skip_count__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.TimeSkippingStatePropagation", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WorkerSelector {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.selector.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.WorkerSelector", len)?;
        if let Some(v) = self.selector.as_ref() {
            match v {
                worker_selector::Selector::WorkerInstanceKey(v) => {
                    struct_ser.serialize_field("workerInstanceKey", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WorkerSelector {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "worker_instance_key",
            "workerInstanceKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            WorkerInstanceKey,
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
                            "workerInstanceKey" | "worker_instance_key" => Ok(GeneratedField::WorkerInstanceKey),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WorkerSelector;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.WorkerSelector")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<WorkerSelector, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut selector__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::WorkerInstanceKey => {
                            if selector__.is_some() {
                                return Err(serde::de::Error::duplicate_field("workerInstanceKey"));
                            }
                            selector__ = map_.next_value::<::std::option::Option<_>>()?.map(worker_selector::Selector::WorkerInstanceKey);
                        }
                    }
                }
                Ok(WorkerSelector {
                    selector: selector__,
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.WorkerSelector", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WorkerVersionCapabilities {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.build_id.is_empty() {
            len += 1;
        }
        if self.use_versioning {
            len += 1;
        }
        if !self.deployment_series_name.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.WorkerVersionCapabilities", len)?;
        if !self.build_id.is_empty() {
            struct_ser.serialize_field("buildId", &self.build_id)?;
        }
        if self.use_versioning {
            struct_ser.serialize_field("useVersioning", &self.use_versioning)?;
        }
        if !self.deployment_series_name.is_empty() {
            struct_ser.serialize_field("deploymentSeriesName", &self.deployment_series_name)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WorkerVersionCapabilities {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "build_id",
            "buildId",
            "use_versioning",
            "useVersioning",
            "deployment_series_name",
            "deploymentSeriesName",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            BuildId,
            UseVersioning,
            DeploymentSeriesName,
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
                            "buildId" | "build_id" => Ok(GeneratedField::BuildId),
                            "useVersioning" | "use_versioning" => Ok(GeneratedField::UseVersioning),
                            "deploymentSeriesName" | "deployment_series_name" => Ok(GeneratedField::DeploymentSeriesName),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WorkerVersionCapabilities;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.WorkerVersionCapabilities")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<WorkerVersionCapabilities, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut build_id__ = None;
                let mut use_versioning__ = None;
                let mut deployment_series_name__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::BuildId => {
                            if build_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("buildId"));
                            }
                            build_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::UseVersioning => {
                            if use_versioning__.is_some() {
                                return Err(serde::de::Error::duplicate_field("useVersioning"));
                            }
                            use_versioning__ = Some(map_.next_value()?);
                        }
                        GeneratedField::DeploymentSeriesName => {
                            if deployment_series_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("deploymentSeriesName"));
                            }
                            deployment_series_name__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(WorkerVersionCapabilities {
                    build_id: build_id__.unwrap_or_default(),
                    use_versioning: use_versioning__.unwrap_or_default(),
                    deployment_series_name: deployment_series_name__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.WorkerVersionCapabilities", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WorkerVersionStamp {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.build_id.is_empty() {
            len += 1;
        }
        if self.use_versioning {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.WorkerVersionStamp", len)?;
        if !self.build_id.is_empty() {
            struct_ser.serialize_field("buildId", &self.build_id)?;
        }
        if self.use_versioning {
            struct_ser.serialize_field("useVersioning", &self.use_versioning)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WorkerVersionStamp {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "build_id",
            "buildId",
            "use_versioning",
            "useVersioning",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            BuildId,
            UseVersioning,
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
                            "buildId" | "build_id" => Ok(GeneratedField::BuildId),
                            "useVersioning" | "use_versioning" => Ok(GeneratedField::UseVersioning),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WorkerVersionStamp;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.WorkerVersionStamp")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<WorkerVersionStamp, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut build_id__ = None;
                let mut use_versioning__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::BuildId => {
                            if build_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("buildId"));
                            }
                            build_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::UseVersioning => {
                            if use_versioning__.is_some() {
                                return Err(serde::de::Error::duplicate_field("useVersioning"));
                            }
                            use_versioning__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(WorkerVersionStamp {
                    build_id: build_id__.unwrap_or_default(),
                    use_versioning: use_versioning__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.WorkerVersionStamp", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WorkflowExecution {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.workflow_id.is_empty() {
            len += 1;
        }
        if !self.run_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.WorkflowExecution", len)?;
        if !self.workflow_id.is_empty() {
            struct_ser.serialize_field("workflowId", &self.workflow_id)?;
        }
        if !self.run_id.is_empty() {
            struct_ser.serialize_field("runId", &self.run_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WorkflowExecution {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "workflow_id",
            "workflowId",
            "run_id",
            "runId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            WorkflowId,
            RunId,
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
                            "workflowId" | "workflow_id" => Ok(GeneratedField::WorkflowId),
                            "runId" | "run_id" => Ok(GeneratedField::RunId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WorkflowExecution;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.WorkflowExecution")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<WorkflowExecution, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut workflow_id__ = None;
                let mut run_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::WorkflowId => {
                            if workflow_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("workflowId"));
                            }
                            workflow_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RunId => {
                            if run_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("runId"));
                            }
                            run_id__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(WorkflowExecution {
                    workflow_id: workflow_id__.unwrap_or_default(),
                    run_id: run_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.WorkflowExecution", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WorkflowType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.name.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("temporal.api.common.v1.WorkflowType", len)?;
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WorkflowType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "name",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Name,
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
                            "name" => Ok(GeneratedField::Name),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WorkflowType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct temporal.api.common.v1.WorkflowType")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<WorkflowType, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut name__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(WorkflowType {
                    name: name__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("temporal.api.common.v1.WorkflowType", FIELDS, GeneratedVisitor)
    }
}
