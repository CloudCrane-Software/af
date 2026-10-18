// Generated from descriptors.bin - DO NOT EDIT
// Payload-limits validators. Edit the *_FIELDS tables in build.rs to classify fields.

#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::batch::v1::BatchOperationReset {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        for (idx, inner) in self.post_reset_operations.iter().enumerate() {
            sink.enter("post_reset_operations", crate::payload_limits::FieldIndexer::Index(idx));
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::batch::v1::BatchOperationSignal {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::batch::v1::BatchOperationTermination {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::cloud::cloudservice::v1::CreateNexusEndpointRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.spec {
            sink.enter("spec", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::cloud::cloudservice::v1::UpdateNexusEndpointRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.spec {
            sink.enter("spec", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::cloud::nexus::v1::EndpointSpec {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::command::v1::CancelWorkflowExecutionCommandAttributes {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::command::v1::Command {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.user_metadata {
            sink.enter("user_metadata", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
        for (idx, inner) in self.event_group_markers.iter().enumerate() {
            sink.enter("event_group_markers", crate::payload_limits::FieldIndexer::Index(idx));
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
        if let Some(oneof) = &self.attributes {
            match oneof {
                crate::protos::temporal::api::command::v1::command::Attributes::ScheduleActivityTaskCommandAttributes(inner) => {
                    sink.enter("schedule_activity_task_command_attributes", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                crate::protos::temporal::api::command::v1::command::Attributes::CompleteWorkflowExecutionCommandAttributes(inner) => {
                    sink.enter("complete_workflow_execution_command_attributes", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                crate::protos::temporal::api::command::v1::command::Attributes::FailWorkflowExecutionCommandAttributes(inner) => {
                    sink.enter("fail_workflow_execution_command_attributes", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                crate::protos::temporal::api::command::v1::command::Attributes::CancelWorkflowExecutionCommandAttributes(inner) => {
                    sink.enter("cancel_workflow_execution_command_attributes", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                crate::protos::temporal::api::command::v1::command::Attributes::RecordMarkerCommandAttributes(inner) => {
                    sink.enter("record_marker_command_attributes", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                crate::protos::temporal::api::command::v1::command::Attributes::ContinueAsNewWorkflowExecutionCommandAttributes(inner) => {
                    sink.enter("continue_as_new_workflow_execution_command_attributes", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                crate::protos::temporal::api::command::v1::command::Attributes::StartChildWorkflowExecutionCommandAttributes(inner) => {
                    sink.enter("start_child_workflow_execution_command_attributes", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                crate::protos::temporal::api::command::v1::command::Attributes::SignalExternalWorkflowExecutionCommandAttributes(inner) => {
                    sink.enter("signal_external_workflow_execution_command_attributes", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                crate::protos::temporal::api::command::v1::command::Attributes::UpsertWorkflowSearchAttributesCommandAttributes(inner) => {
                    sink.enter("upsert_workflow_search_attributes_command_attributes", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                crate::protos::temporal::api::command::v1::command::Attributes::ModifyWorkflowPropertiesCommandAttributes(inner) => {
                    sink.enter("modify_workflow_properties_command_attributes", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                crate::protos::temporal::api::command::v1::command::Attributes::ScheduleNexusOperationCommandAttributes(inner) => {
                    sink.enter("schedule_nexus_operation_command_attributes", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                _ => {}
            }
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::command::v1::CompleteWorkflowExecutionCommandAttributes {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.result {
            sink.check("result", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::command::v1::ContinueAsNewWorkflowExecutionCommandAttributes {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.input {
            sink.check("input", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
        if let Some(inner) = &self.memo {
            sink.check("memo", crate::payload_limits::LimitClass::Memo, crate::payload_limits::memo_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::command::v1::FailWorkflowExecutionCommandAttributes {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.failure {
            sink.check("failure", crate::payload_limits::LimitClass::Blob, crate::payload_limits::message_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::command::v1::ModifyWorkflowPropertiesCommandAttributes {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.upserted_memo {
            sink.check("upserted_memo", crate::payload_limits::LimitClass::Blob, crate::payload_limits::map_payload_data_sum(inner.fields.iter()), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::command::v1::RecordMarkerCommandAttributes {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        sink.check("details", crate::payload_limits::LimitClass::Blob, crate::payload_limits::map_payloads_sum(self.details.iter()), true);
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::command::v1::ScheduleActivityTaskCommandAttributes {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.input {
            sink.check("input", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::command::v1::ScheduleNexusOperationCommandAttributes {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.input {
            sink.check("input", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payload_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::command::v1::SignalExternalWorkflowExecutionCommandAttributes {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.input {
            sink.check("input", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::command::v1::StartChildWorkflowExecutionCommandAttributes {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.input {
            sink.check("input", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
        if let Some(inner) = &self.memo {
            sink.check("memo", crate::payload_limits::LimitClass::Memo, crate::payload_limits::memo_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::command::v1::UpsertWorkflowSearchAttributesCommandAttributes {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.search_attributes {
            sink.check("search_attributes", crate::payload_limits::LimitClass::Blob, crate::payload_limits::map_payload_data_sum(inner.indexed_fields.iter()), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::common::v1::Callback {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(oneof) = &self.variant {
            match oneof {
                crate::protos::temporal::api::common::v1::callback::Variant::NexusHandler(inner) => {
                    sink.enter("nexus_handler", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                _ => {}
            }
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::common::v1::callback::NexusHandler {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.source_context {
            sink.check("source_context", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payload_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::compute::v1::ComputeConfig {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        for (key, inner) in self.scaling_groups.iter() {
            sink.enter("scaling_groups", crate::payload_limits::FieldIndexer::Key(key));
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::compute::v1::ComputeConfigScalingGroup {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.provider {
            sink.enter("provider", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
        if let Some(inner) = &self.scaler {
            sink.enter("scaler", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::compute::v1::ComputeConfigScalingGroupUpdate {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.scaling_group {
            sink.enter("scaling_group", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::compute::v1::ComputeProvider {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::compute::v1::ComputeScaler {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::deployment::v1::UpdateDeploymentMetadata {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::nexus::v1::EndpointSpec {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::nexus::v1::Response {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(oneof) = &self.variant {
            match oneof {
                crate::protos::temporal::api::nexus::v1::response::Variant::StartOperation(inner) => {
                    sink.enter("start_operation", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                _ => {}
            }
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::nexus::v1::StartOperationResponse {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(oneof) = &self.variant {
            match oneof {
                crate::protos::temporal::api::nexus::v1::start_operation_response::Variant::SyncSuccess(inner) => {
                    sink.enter("sync_success", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                crate::protos::temporal::api::nexus::v1::start_operation_response::Variant::Failure(inner) => {  }
                _ => {}
            }
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::nexus::v1::start_operation_response::Sync {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::operatorservice::v1::CreateNexusEndpointRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.spec {
            sink.enter("spec", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::operatorservice::v1::UpdateNexusEndpointRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.spec {
            sink.enter("spec", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::protocol::v1::Message {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.body {
            sink.check("body", crate::payload_limits::LimitClass::Blob, crate::payload_limits::message_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::query::v1::WorkflowQuery {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.query_args {
            sink.check("query_args", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::query::v1::WorkflowQueryResult {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.answer {
            sink.check("answer", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), false);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::schedule::v1::Schedule {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.action {
            sink.enter("action", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::schedule::v1::ScheduleAction {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(oneof) = &self.action {
            match oneof {
                crate::protos::temporal::api::schedule::v1::schedule_action::Action::StartWorkflow(inner) => {
                    sink.enter("start_workflow", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
            }
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::sdk::v1::EventGroupMarker {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(oneof) = &self.variant {
            match oneof {
                crate::protos::temporal::api::sdk::v1::event_group_marker::Variant::Label(inner) => {
                    sink.enter("label", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                _ => {}
            }
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::sdk::v1::event_group_marker::Label {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::sdk::v1::UserMetadata {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::update::v1::Input {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::update::v1::Request {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.input {
            sink.enter("input", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
        for (idx, inner) in self.completion_callbacks.iter().enumerate() {
            sink.enter("completion_callbacks", crate::payload_limits::FieldIndexer::Index(idx));
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflow::v1::NewWorkflowExecutionInfo {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.input {
            sink.check("input", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
        if let Some(inner) = &self.memo {
            sink.check("memo", crate::payload_limits::LimitClass::Memo, crate::payload_limits::memo_size(inner), true);
        }
        if let Some(inner) = &self.user_metadata {
            sink.enter("user_metadata", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflow::v1::PostResetOperation {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(oneof) = &self.variant {
            match oneof {
                crate::protos::temporal::api::workflow::v1::post_reset_operation::Variant::SignalWorkflow(inner) => {
                    sink.enter("signal_workflow", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                _ => {}
            }
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflow::v1::post_reset_operation::SignalWorkflow {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::CreateScheduleRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.schedule {
            sink.enter("schedule", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::CreateWorkerDeploymentVersionRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.compute_config {
            sink.enter("compute_config", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::ExecuteMultiOperationRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        for (idx, inner) in self.operations.iter().enumerate() {
            sink.enter("operations", crate::payload_limits::FieldIndexer::Index(idx));
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::execute_multi_operation_request::Operation {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(oneof) = &self.operation {
            match oneof {
                crate::protos::temporal::api::workflowservice::v1::execute_multi_operation_request::operation::Operation::StartWorkflow(inner) => {
                    sink.enter("start_workflow", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                crate::protos::temporal::api::workflowservice::v1::execute_multi_operation_request::operation::Operation::UpdateWorkflow(inner) => {
                    sink.enter("update_workflow", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
            }
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::QueryWorkflowRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.query {
            sink.enter("query", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::RecordActivityTaskHeartbeatByIdRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.details {
            sink.check("details", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::RecordActivityTaskHeartbeatRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.details {
            sink.check("details", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::ResetWorkflowExecutionRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        for (idx, inner) in self.post_reset_operations.iter().enumerate() {
            sink.enter("post_reset_operations", crate::payload_limits::FieldIndexer::Index(idx));
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskCanceledByIdRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.details {
            sink.check("details", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskCanceledRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.details {
            sink.check("details", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskCompletedByIdRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.result {
            sink.check("result", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskCompletedRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.result {
            sink.check("result", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskFailedByIdRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.failure {
            sink.check("failure", crate::payload_limits::LimitClass::Blob, crate::payload_limits::message_size(inner), false);
        }
        if let Some(inner) = &self.last_heartbeat_details {
            sink.check("last_heartbeat_details", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), false);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskFailedRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.failure {
            sink.check("failure", crate::payload_limits::LimitClass::Blob, crate::payload_limits::message_size(inner), false);
        }
        if let Some(inner) = &self.last_heartbeat_details {
            sink.check("last_heartbeat_details", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), false);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::RespondNexusTaskCompletedRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.response {
            sink.enter("response", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::RespondNexusTaskFailedRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.failure {
            sink.check("failure", crate::payload_limits::LimitClass::Blob, crate::payload_limits::message_size(inner), false);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::RespondQueryTaskCompletedRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.query_result {
            sink.check("query_result", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), false);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::RespondWorkflowTaskCompletedRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        for (idx, inner) in self.commands.iter().enumerate() {
            sink.enter("commands", crate::payload_limits::FieldIndexer::Index(idx));
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
        for (key, inner) in self.query_results.iter() {
            sink.enter("query_results", crate::payload_limits::FieldIndexer::Key(key));
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
        for (idx, inner) in self.messages.iter().enumerate() {
            sink.enter("messages", crate::payload_limits::FieldIndexer::Index(idx));
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::RespondWorkflowTaskFailedRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.failure {
            sink.check("failure", crate::payload_limits::LimitClass::Blob, crate::payload_limits::message_size(inner), false);
        }
        for (idx, inner) in self.messages.iter().enumerate() {
            sink.enter("messages", crate::payload_limits::FieldIndexer::Index(idx));
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::SetCurrentDeploymentRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.update_metadata {
            sink.enter("update_metadata", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::SignalWithStartWorkflowExecutionRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.input {
            sink.check("input", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
        if let Some(inner) = &self.signal_input {
            sink.check("signal_input", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
        if let Some(inner) = &self.memo {
            sink.check("memo", crate::payload_limits::LimitClass::Memo, crate::payload_limits::memo_size(inner), true);
        }
        if let Some(inner) = &self.user_metadata {
            sink.enter("user_metadata", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::SignalWorkflowExecutionRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.input {
            sink.check("input", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::StartActivityExecutionRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.input {
            sink.check("input", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
        if let Some(inner) = &self.user_metadata {
            sink.enter("user_metadata", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
        for (idx, inner) in self.completion_callbacks.iter().enumerate() {
            sink.enter("completion_callbacks", crate::payload_limits::FieldIndexer::Index(idx));
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::StartBatchOperationRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(oneof) = &self.operation {
            match oneof {
                crate::protos::temporal::api::workflowservice::v1::start_batch_operation_request::Operation::TerminationOperation(inner) => {
                    sink.enter("termination_operation", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                crate::protos::temporal::api::workflowservice::v1::start_batch_operation_request::Operation::SignalOperation(inner) => {
                    sink.enter("signal_operation", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                crate::protos::temporal::api::workflowservice::v1::start_batch_operation_request::Operation::ResetOperation(inner) => {
                    sink.enter("reset_operation", crate::payload_limits::FieldIndexer::None);
                    crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
                    sink.exit();
                }
                _ => {}
            }
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::StartNexusOperationExecutionRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.input {
            sink.check("input", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payload_size(inner), true);
        }
        if let Some(inner) = &self.user_metadata {
            sink.enter("user_metadata", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
        for (idx, inner) in self.completion_callbacks.iter().enumerate() {
            sink.enter("completion_callbacks", crate::payload_limits::FieldIndexer::Index(idx));
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::StartWorkflowExecutionRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.input {
            sink.check("input", crate::payload_limits::LimitClass::Blob, crate::payload_limits::payloads_size(inner), true);
        }
        if let Some(inner) = &self.memo {
            sink.check("memo", crate::payload_limits::LimitClass::Memo, crate::payload_limits::memo_size(inner), true);
        }
        for (idx, inner) in self.completion_callbacks.iter().enumerate() {
            sink.enter("completion_callbacks", crate::payload_limits::FieldIndexer::Index(idx));
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
        if let Some(inner) = &self.user_metadata {
            sink.enter("user_metadata", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::TerminateWorkflowExecutionRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::UpdateScheduleRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.schedule {
            sink.enter("schedule", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::UpdateWorkerDeploymentVersionComputeConfigRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        for (key, inner) in self.compute_config_scaling_groups.iter() {
            sink.enter("compute_config_scaling_groups", crate::payload_limits::FieldIndexer::Key(key));
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::UpdateWorkerDeploymentVersionMetadataRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::UpdateWorkflowExecutionRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        if let Some(inner) = &self.request {
            sink.enter("request", crate::payload_limits::FieldIndexer::None);
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}
#[allow(deprecated, unused_variables, clippy::collapsible_if, clippy::collapsible_match, clippy::single_match)]
impl crate::payload_limits::PayloadLimitsValidatable for crate::protos::temporal::api::workflowservice::v1::ValidateWorkerDeploymentVersionComputeConfigRequest {
    fn validate_payload_limits(&self, sink: &mut dyn crate::payload_limits::PayloadLimitSink) {
        for (key, inner) in self.compute_config_scaling_groups.iter() {
            sink.enter("compute_config_scaling_groups", crate::payload_limits::FieldIndexer::Key(key));
            crate::payload_limits::PayloadLimitsValidatable::validate_payload_limits(inner, sink);
            sink.exit();
        }
    }
}

/// Validate `req` if it is a known payload-bearing request type; otherwise return `None`.
#[doc(hidden)]
pub fn validate_known_payload_limits(
    req: &dyn ::std::any::Any,
    limits: &PayloadLimits,
) -> Option<PayloadLimitViolation> {
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::cloud::cloudservice::v1::CreateNexusEndpointRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::cloud::cloudservice::v1::UpdateNexusEndpointRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::operatorservice::v1::CreateNexusEndpointRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::operatorservice::v1::UpdateNexusEndpointRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::CreateScheduleRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::CreateWorkerDeploymentVersionRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::ExecuteMultiOperationRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::QueryWorkflowRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::RecordActivityTaskHeartbeatByIdRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::RecordActivityTaskHeartbeatRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::ResetWorkflowExecutionRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskCanceledByIdRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskCanceledRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskCompletedByIdRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskCompletedRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskFailedByIdRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskFailedRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::RespondNexusTaskCompletedRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::RespondNexusTaskFailedRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::RespondQueryTaskCompletedRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::RespondWorkflowTaskCompletedRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::RespondWorkflowTaskFailedRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::SetCurrentDeploymentRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::SignalWithStartWorkflowExecutionRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::SignalWorkflowExecutionRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::StartActivityExecutionRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::StartBatchOperationRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::StartNexusOperationExecutionRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::StartWorkflowExecutionRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::TerminateWorkflowExecutionRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::UpdateScheduleRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::UpdateWorkerDeploymentVersionComputeConfigRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::UpdateWorkerDeploymentVersionMetadataRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::UpdateWorkflowExecutionRequest>() {
        return validate_payload_limits(inner, limits);
    }
    if let Some(inner) = req.downcast_ref::<crate::protos::temporal::api::workflowservice::v1::ValidateWorkerDeploymentVersionComputeConfigRequest>() {
        return validate_payload_limits(inner, limits);
    }
    None
}
