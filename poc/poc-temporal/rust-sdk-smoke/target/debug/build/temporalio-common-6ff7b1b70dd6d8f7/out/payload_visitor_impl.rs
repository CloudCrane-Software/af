// Generated from descriptors.bin - DO NOT EDIT

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::ActivityHeartbeat {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "coresdk.ActivityHeartbeat.details",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.details),
        }).await?;
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::ActivityTaskCompletion {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.result {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::activity_result::ActivityExecutionResult {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(status) = &mut self.status {
            match status {
                crate::protos::coresdk::activity_result::activity_execution_result::Status::Completed(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::activity_result::activity_execution_result::Status::Failed(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::activity_result::activity_execution_result::Status::Cancelled(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::activity_result::ActivityResolution {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(status) = &mut self.status {
            match status {
                crate::protos::coresdk::activity_result::activity_resolution::Status::Completed(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::activity_result::activity_resolution::Status::Failed(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::activity_result::activity_resolution::Status::Cancelled(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::activity_result::Cancellation {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::activity_result::Failure {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::activity_result::Success {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.activity_result.Success.result",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::activity_task::ActivityTask {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(variant) = &mut self.variant {
            match variant {
                crate::protos::coresdk::activity_task::activity_task::Variant::Start(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::activity_task::Start {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for payload in self.header_fields.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.activity_task.Start.header_fields",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "coresdk.activity_task.Start.input",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.input),
        }).await?;
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "coresdk.activity_task.Start.heartbeat_details",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.heartbeat_details),
        }).await?;
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::child_workflow::Cancellation {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::child_workflow::ChildWorkflowResult {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(status) = &mut self.status {
            match status {
                crate::protos::coresdk::child_workflow::child_workflow_result::Status::Completed(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::child_workflow::child_workflow_result::Status::Failed(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::child_workflow::child_workflow_result::Status::Cancelled(msg) => msg.visit_payloads_mut(visitor).await?,
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::child_workflow::Failure {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::child_workflow::Success {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.child_workflow.Success.result",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::nexus::NexusOperationResult {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(status) = &mut self.status {
            match status {
                crate::protos::coresdk::nexus::nexus_operation_result::Status::Completed(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::nexus::nexus_operation_result::Status::Failed(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::nexus::nexus_operation_result::Status::Cancelled(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::nexus::nexus_operation_result::Status::TimedOut(msg) => msg.visit_payloads_mut(visitor).await?,
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::nexus::NexusTask {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(variant) = &mut self.variant {
            match variant {
                crate::protos::coresdk::nexus::nexus_task::Variant::Task(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::nexus::NexusTaskCompletion {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(status) = &mut self.status {
            match status {
                crate::protos::coresdk::nexus::nexus_task_completion::Status::Completed(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::nexus::nexus_task_completion::Status::Failure(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_activation::DoUpdate {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "coresdk.workflow_activation.DoUpdate.input",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.input),
        }).await?;
        for payload in self.headers.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_activation.DoUpdate.headers",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_activation::InitializeWorkflow {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "coresdk.workflow_activation.InitializeWorkflow.arguments",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.arguments),
        }).await?;
        for payload in self.headers.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_activation.InitializeWorkflow.headers",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
        if let Some(msg) = &mut self.continued_failure {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(payloads) = &mut self.last_completion_result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_activation.InitializeWorkflow.last_completion_result",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.memo {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_activation::QueryWorkflow {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "coresdk.workflow_activation.QueryWorkflow.arguments",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.arguments),
        }).await?;
        for payload in self.headers.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_activation.QueryWorkflow.headers",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_activation::ResolveActivity {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.result {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_activation::ResolveChildWorkflowExecution {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.result {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_activation::ResolveChildWorkflowExecutionStart {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(status) = &mut self.status {
            match status {
                crate::protos::coresdk::workflow_activation::resolve_child_workflow_execution_start::Status::Cancelled(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_activation::ResolveChildWorkflowExecutionStartCancelled {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_activation::ResolveNexusOperation {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.result {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_activation::ResolveNexusOperationStart {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(status) = &mut self.status {
            match status {
                crate::protos::coresdk::workflow_activation::resolve_nexus_operation_start::Status::Failed(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_activation::ResolveRequestCancelExternalWorkflow {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_activation::ResolveSignalExternalWorkflow {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_activation::SignalWorkflow {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "coresdk.workflow_activation.SignalWorkflow.input",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.input),
        }).await?;
        for payload in self.headers.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_activation.SignalWorkflow.headers",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_activation::WorkflowActivation {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.jobs {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_activation::WorkflowActivationJob {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(variant) = &mut self.variant {
            match variant {
                crate::protos::coresdk::workflow_activation::workflow_activation_job::Variant::InitializeWorkflow(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_activation::workflow_activation_job::Variant::QueryWorkflow(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_activation::workflow_activation_job::Variant::SignalWorkflow(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_activation::workflow_activation_job::Variant::ResolveActivity(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_activation::workflow_activation_job::Variant::ResolveChildWorkflowExecutionStart(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_activation::workflow_activation_job::Variant::ResolveChildWorkflowExecution(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_activation::workflow_activation_job::Variant::ResolveSignalExternalWorkflow(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_activation::workflow_activation_job::Variant::ResolveRequestCancelExternalWorkflow(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_activation::workflow_activation_job::Variant::DoUpdate(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_activation::workflow_activation_job::Variant::ResolveNexusOperationStart(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_activation::workflow_activation_job::Variant::ResolveNexusOperation(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_commands::CancelWorkflowExecution {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_commands.CancelWorkflowExecution.details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_commands::CompleteWorkflowExecution {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_commands.CompleteWorkflowExecution.result",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_commands::ContinueAsNewWorkflowExecution {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "coresdk.workflow_commands.ContinueAsNewWorkflowExecution.arguments",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.arguments),
        }).await?;
        for payload in self.memo.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_commands.ContinueAsNewWorkflowExecution.memo",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
        for payload in self.headers.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_commands.ContinueAsNewWorkflowExecution.headers",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_commands::FailWorkflowExecution {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_commands::ModifyWorkflowProperties {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.upserted_memo {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_commands::QueryResult {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(variant) = &mut self.variant {
            match variant {
                crate::protos::coresdk::workflow_commands::query_result::Variant::Succeeded(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_commands::query_result::Variant::Failed(msg) => msg.visit_payloads_mut(visitor).await?,
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_commands::QuerySuccess {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.response {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_commands.QuerySuccess.response",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_commands::ScheduleActivity {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for payload in self.headers.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_commands.ScheduleActivity.headers",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "coresdk.workflow_commands.ScheduleActivity.arguments",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.arguments),
        }).await?;
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_commands::ScheduleLocalActivity {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for payload in self.headers.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_commands.ScheduleLocalActivity.headers",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "coresdk.workflow_commands.ScheduleLocalActivity.arguments",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.arguments),
        }).await?;
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_commands::ScheduleNexusOperation {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_commands.ScheduleNexusOperation.input",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_commands::SignalExternalWorkflowExecution {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "coresdk.workflow_commands.SignalExternalWorkflowExecution.args",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.args),
        }).await?;
        for payload in self.headers.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_commands.SignalExternalWorkflowExecution.headers",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_commands::StartChildWorkflowExecution {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "coresdk.workflow_commands.StartChildWorkflowExecution.input",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.input),
        }).await?;
        for payload in self.headers.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_commands.StartChildWorkflowExecution.headers",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
        for payload in self.memo.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "coresdk.workflow_commands.StartChildWorkflowExecution.memo",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_commands::UpdateResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(response) = &mut self.response {
            match response {
                crate::protos::coresdk::workflow_commands::update_response::Response::Rejected(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_commands::update_response::Response::Completed(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_commands::UpsertWorkflowSearchAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_commands::WorkflowCommand {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.user_metadata {
            msg.visit_payloads_mut(visitor).await?;
        }
        for item in &mut self.event_group_markers {
            item.visit_payloads_mut(visitor).await?;
        }
        if let Some(variant) = &mut self.variant {
            match variant {
                crate::protos::coresdk::workflow_commands::workflow_command::Variant::ScheduleActivity(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_commands::workflow_command::Variant::RespondToQuery(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_commands::workflow_command::Variant::CompleteWorkflowExecution(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_commands::workflow_command::Variant::FailWorkflowExecution(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_commands::workflow_command::Variant::ContinueAsNewWorkflowExecution(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_commands::workflow_command::Variant::CancelWorkflowExecution(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_commands::workflow_command::Variant::StartChildWorkflowExecution(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_commands::workflow_command::Variant::SignalExternalWorkflowExecution(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_commands::workflow_command::Variant::ScheduleLocalActivity(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_commands::workflow_command::Variant::UpsertWorkflowSearchAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_commands::workflow_command::Variant::ModifyWorkflowProperties(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_commands::workflow_command::Variant::UpdateResponse(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_commands::workflow_command::Variant::ScheduleNexusOperation(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_completion::Failure {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_completion::Success {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.commands {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::coresdk::workflow_completion::WorkflowActivationCompletion {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(status) = &mut self.status {
            match status {
                crate::protos::coresdk::workflow_completion::workflow_activation_completion::Status::Successful(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::coresdk::workflow_completion::workflow_activation_completion::Status::Failed(msg) => msg.visit_payloads_mut(visitor).await?,
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::activity::v1::ActivityExecutionInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.heartbeat_details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.activity.v1.ActivityExecutionInfo.heartbeat_details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.last_failure {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.user_metadata {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::activity::v1::ActivityExecutionListInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::activity::v1::ActivityExecutionOutcome {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(value) = &mut self.value {
            match value {
                crate::protos::temporal::api::activity::v1::activity_execution_outcome::Value::Result(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::activity::v1::activity_execution_outcome::Value::Failure(msg) => msg.visit_payloads_mut(visitor).await?,
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::activity::v1::CallbackInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.info {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::batch::v1::BatchOperationReset {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.post_reset_operations {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::batch::v1::BatchOperationSignal {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.batch.v1.BatchOperationSignal.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::batch::v1::BatchOperationTermination {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.batch.v1.BatchOperationTermination.details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::callback::v1::CallbackInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.callback {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.last_attempt_failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::cloud::cloudservice::v1::CreateNexusEndpointRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.spec {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::cloud::cloudservice::v1::GetNexusEndpointResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.endpoint {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::cloud::cloudservice::v1::GetNexusEndpointsResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.endpoints {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::cloud::cloudservice::v1::UpdateNexusEndpointRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.spec {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::cloud::nexus::v1::Endpoint {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.spec {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::cloud::nexus::v1::EndpointSpec {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.description {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.cloud.nexus.v1.EndpointSpec.description",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::command::v1::CancelWorkflowExecutionCommandAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.command.v1.CancelWorkflowExecutionCommandAttributes.details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::command::v1::Command {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.user_metadata {
            msg.visit_payloads_mut(visitor).await?;
        }
        for item in &mut self.event_group_markers {
            item.visit_payloads_mut(visitor).await?;
        }
        if let Some(attributes) = &mut self.attributes {
            match attributes {
                crate::protos::temporal::api::command::v1::command::Attributes::ScheduleActivityTaskCommandAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::command::v1::command::Attributes::CompleteWorkflowExecutionCommandAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::command::v1::command::Attributes::FailWorkflowExecutionCommandAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::command::v1::command::Attributes::CancelWorkflowExecutionCommandAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::command::v1::command::Attributes::RecordMarkerCommandAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::command::v1::command::Attributes::ContinueAsNewWorkflowExecutionCommandAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::command::v1::command::Attributes::StartChildWorkflowExecutionCommandAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::command::v1::command::Attributes::SignalExternalWorkflowExecutionCommandAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::command::v1::command::Attributes::UpsertWorkflowSearchAttributesCommandAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::command::v1::command::Attributes::ModifyWorkflowPropertiesCommandAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::command::v1::command::Attributes::ScheduleNexusOperationCommandAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::command::v1::CompleteWorkflowExecutionCommandAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.command.v1.CompleteWorkflowExecutionCommandAttributes.result",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::command::v1::ContinueAsNewWorkflowExecutionCommandAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.command.v1.ContinueAsNewWorkflowExecutionCommandAttributes.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(payloads) = &mut self.last_completion_result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.command.v1.ContinueAsNewWorkflowExecutionCommandAttributes.last_completion_result",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.memo {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::command::v1::FailWorkflowExecutionCommandAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::command::v1::ModifyWorkflowPropertiesCommandAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.upserted_memo {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::command::v1::RecordMarkerCommandAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in self.details.values_mut() {
            item.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::command::v1::ScheduleActivityTaskCommandAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.command.v1.ScheduleActivityTaskCommandAttributes.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::command::v1::ScheduleNexusOperationCommandAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.command.v1.ScheduleNexusOperationCommandAttributes.input",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::command::v1::SignalExternalWorkflowExecutionCommandAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.command.v1.SignalExternalWorkflowExecutionCommandAttributes.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::command::v1::StartChildWorkflowExecutionCommandAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.command.v1.StartChildWorkflowExecutionCommandAttributes.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.memo {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::command::v1::UpsertWorkflowSearchAttributesCommandAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::common::v1::Callback {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(variant) = &mut self.variant {
            match variant {
                crate::protos::temporal::api::common::v1::callback::Variant::NexusHandler(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::common::v1::callback::NexusHandler {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.source_context {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.common.v1.Callback.NexusHandler.source_context",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::common::v1::Header {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for payload in self.fields.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.common.v1.Header.fields",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::common::v1::Memo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for payload in self.fields.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.common.v1.Memo.fields",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::common::v1::SearchAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for payload in self.indexed_fields.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.common.v1.SearchAttributes.indexed_fields",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::compute::v1::ComputeConfig {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in self.scaling_groups.values_mut() {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::compute::v1::ComputeConfigScalingGroup {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.provider {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.scaler {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::compute::v1::ComputeConfigScalingGroupUpdate {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.scaling_group {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::compute::v1::ComputeProvider {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.compute.v1.ComputeProvider.details",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::compute::v1::ComputeScaler {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.compute.v1.ComputeScaler.details",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::deployment::v1::DeploymentInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for payload in self.metadata.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.deployment.v1.DeploymentInfo.metadata",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::deployment::v1::UpdateDeploymentMetadata {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for payload in self.upsert_entries.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.deployment.v1.UpdateDeploymentMetadata.upsert_entries",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::deployment::v1::VersionMetadata {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for payload in self.entries.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.deployment.v1.VersionMetadata.entries",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::deployment::v1::WorkerDeploymentVersionInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.metadata {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.compute_config {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::errordetails::v1::QueryFailedFailure {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::failure::v1::ApplicationFailureInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.failure.v1.ApplicationFailureInfo.details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::failure::v1::CanceledFailureInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.failure.v1.CanceledFailureInfo.details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::failure::v1::Failure {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.encoded_attributes {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.failure.v1.Failure.encoded_attributes",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
        if let Some(msg) = &mut self.cause {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(failure_info) = &mut self.failure_info {
            match failure_info {
                crate::protos::temporal::api::failure::v1::failure::FailureInfo::ApplicationFailureInfo(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::failure::v1::failure::FailureInfo::TimeoutFailureInfo(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::failure::v1::failure::FailureInfo::CanceledFailureInfo(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::failure::v1::failure::FailureInfo::ResetWorkflowFailureInfo(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::failure::v1::ResetWorkflowFailureInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.last_heartbeat_details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.failure.v1.ResetWorkflowFailureInfo.last_heartbeat_details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::failure::v1::TimeoutFailureInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.last_heartbeat_details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.failure.v1.TimeoutFailureInfo.last_heartbeat_details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::ActivityTaskCanceledEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.ActivityTaskCanceledEventAttributes.details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::ActivityTaskCompletedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.ActivityTaskCompletedEventAttributes.result",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::ActivityTaskFailedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::ActivityTaskScheduledEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.ActivityTaskScheduledEventAttributes.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::ActivityTaskStartedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.last_failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::ActivityTaskTimedOutEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::ChildWorkflowExecutionCanceledEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.ChildWorkflowExecutionCanceledEventAttributes.details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::ChildWorkflowExecutionCompletedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.ChildWorkflowExecutionCompletedEventAttributes.result",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::ChildWorkflowExecutionFailedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::ChildWorkflowExecutionStartedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::History {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.events {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::HistoryEvent {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.user_metadata {
            msg.visit_payloads_mut(visitor).await?;
        }
        for item in &mut self.event_group_markers {
            item.visit_payloads_mut(visitor).await?;
        }
        if let Some(attributes) = &mut self.attributes {
            match attributes {
                crate::protos::temporal::api::history::v1::history_event::Attributes::WorkflowExecutionStartedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::WorkflowExecutionCompletedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::WorkflowExecutionFailedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::WorkflowTaskFailedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::ActivityTaskScheduledEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::ActivityTaskStartedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::ActivityTaskCompletedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::ActivityTaskFailedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::ActivityTaskTimedOutEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::ActivityTaskCanceledEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::MarkerRecordedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::WorkflowExecutionSignaledEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::WorkflowExecutionTerminatedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::WorkflowExecutionCanceledEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::WorkflowExecutionContinuedAsNewEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::StartChildWorkflowExecutionInitiatedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::ChildWorkflowExecutionStartedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::ChildWorkflowExecutionCompletedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::ChildWorkflowExecutionFailedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::ChildWorkflowExecutionCanceledEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::SignalExternalWorkflowExecutionInitiatedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::UpsertWorkflowSearchAttributesEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::WorkflowExecutionUpdateAcceptedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::WorkflowExecutionUpdateRejectedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::WorkflowExecutionUpdateCompletedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::WorkflowPropertiesModifiedExternallyEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::WorkflowPropertiesModifiedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::WorkflowExecutionUpdateAdmittedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::NexusOperationScheduledEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::NexusOperationCompletedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::NexusOperationFailedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::NexusOperationCanceledEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::NexusOperationTimedOutEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::WorkflowExecutionOptionsUpdatedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::history::v1::history_event::Attributes::NexusOperationCancelRequestFailedEventAttributes(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::MarkerRecordedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in self.details.values_mut() {
            item.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::NexusOperationCancelRequestFailedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::NexusOperationCanceledEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::NexusOperationCompletedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.NexusOperationCompletedEventAttributes.result",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::NexusOperationFailedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::NexusOperationScheduledEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.NexusOperationScheduledEventAttributes.input",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::NexusOperationTimedOutEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::SignalExternalWorkflowExecutionInitiatedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.SignalExternalWorkflowExecutionInitiatedEventAttributes.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::StartChildWorkflowExecutionInitiatedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.StartChildWorkflowExecutionInitiatedEventAttributes.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.memo {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::UpsertWorkflowSearchAttributesEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::WorkflowExecutionCanceledEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.WorkflowExecutionCanceledEventAttributes.details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::WorkflowExecutionCompletedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.WorkflowExecutionCompletedEventAttributes.result",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::WorkflowExecutionContinuedAsNewEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.WorkflowExecutionContinuedAsNewEventAttributes.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(payloads) = &mut self.last_completion_result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.WorkflowExecutionContinuedAsNewEventAttributes.last_completion_result",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.memo {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::WorkflowExecutionFailedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::WorkflowExecutionOptionsUpdatedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.attached_completion_callbacks {
            item.visit_payloads_mut(visitor).await?;
        }
        for item in &mut self.workflow_update_options {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::workflow_execution_options_updated_event_attributes::WorkflowUpdateOptionsUpdate {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.attached_completion_callbacks {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::WorkflowExecutionSignaledEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.WorkflowExecutionSignaledEventAttributes.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::WorkflowExecutionStartedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.WorkflowExecutionStartedEventAttributes.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.continued_failure {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(payloads) = &mut self.last_completion_result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.WorkflowExecutionStartedEventAttributes.last_completion_result",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.memo {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        for item in &mut self.completion_callbacks {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::WorkflowExecutionTerminatedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.history.v1.WorkflowExecutionTerminatedEventAttributes.details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::WorkflowExecutionUpdateAcceptedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.accepted_request {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::WorkflowExecutionUpdateAdmittedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.request {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::WorkflowExecutionUpdateCompletedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.outcome {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::WorkflowExecutionUpdateRejectedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.rejected_request {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::WorkflowPropertiesModifiedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.upserted_memo {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::WorkflowPropertiesModifiedExternallyEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.upserted_memo {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::history::v1::WorkflowTaskFailedEventAttributes {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::nexus::v1::Endpoint {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.spec {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::nexus::v1::EndpointSpec {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.description {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.nexus.v1.EndpointSpec.description",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::nexus::v1::NexusOperationExecutionCancellationInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.last_attempt_failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::nexus::v1::NexusOperationExecutionInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.last_attempt_failure {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.cancellation_info {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.user_metadata {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::nexus::v1::NexusOperationExecutionListInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::nexus::v1::Request {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(variant) = &mut self.variant {
            match variant {
                crate::protos::temporal::api::nexus::v1::request::Variant::StartOperation(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::nexus::v1::Response {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(variant) = &mut self.variant {
            match variant {
                crate::protos::temporal::api::nexus::v1::response::Variant::StartOperation(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::nexus::v1::StartOperationRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.payload {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.nexus.v1.StartOperationRequest.payload",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::nexus::v1::StartOperationResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(variant) = &mut self.variant {
            match variant {
                crate::protos::temporal::api::nexus::v1::start_operation_response::Variant::SyncSuccess(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::nexus::v1::start_operation_response::Variant::Failure(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::nexus::v1::start_operation_response::Sync {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.payload {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.nexus.v1.StartOperationResponse.Sync.payload",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::nexusoperation::v1::CallbackInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.info {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::operatorservice::v1::CreateNexusEndpointRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.spec {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::operatorservice::v1::CreateNexusEndpointResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.endpoint {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::operatorservice::v1::GetNexusEndpointResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.endpoint {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::operatorservice::v1::ListNexusEndpointsResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.endpoints {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::operatorservice::v1::UpdateNexusEndpointRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.spec {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::operatorservice::v1::UpdateNexusEndpointResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.endpoint {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::query::v1::WorkflowQuery {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.query_args {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.query.v1.WorkflowQuery.query_args",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::query::v1::WorkflowQueryResult {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.answer {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.query.v1.WorkflowQueryResult.answer",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::schedule::v1::Schedule {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.action {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::schedule::v1::ScheduleAction {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(action) = &mut self.action {
            match action {
                crate::protos::temporal::api::schedule::v1::schedule_action::Action::StartWorkflow(msg) => msg.visit_payloads_mut(visitor).await?,
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::schedule::v1::ScheduleListEntry {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.memo {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::sdk::v1::EventGroupMarker {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(variant) = &mut self.variant {
            match variant {
                crate::protos::temporal::api::sdk::v1::event_group_marker::Variant::Label(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::sdk::v1::event_group_marker::Label {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.label {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.sdk.v1.EventGroupMarker.Label.label",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::sdk::v1::UserMetadata {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.summary {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.sdk.v1.UserMetadata.summary",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
        if let Some(payload) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.sdk.v1.UserMetadata.details",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::update::v1::Acceptance {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.accepted_request {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::update::v1::Input {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(payloads) = &mut self.args {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.update.v1.Input.args",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::update::v1::Outcome {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(value) = &mut self.value {
            match value {
                crate::protos::temporal::api::update::v1::outcome::Value::Success(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::update::v1::outcome::Value::Failure(msg) => msg.visit_payloads_mut(visitor).await?,
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::update::v1::Rejection {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.rejected_request {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::update::v1::Request {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.input {
            msg.visit_payloads_mut(visitor).await?;
        }
        for item in &mut self.completion_callbacks {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::update::v1::Response {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.outcome {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflow::v1::CallbackInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.callback {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.last_attempt_failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflow::v1::NewWorkflowExecutionInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflow.v1.NewWorkflowExecutionInfo.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.memo {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.user_metadata {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflow::v1::NexusOperationCancellationInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.last_attempt_failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflow::v1::PendingActivityInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.heartbeat_details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflow.v1.PendingActivityInfo.heartbeat_details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.last_failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflow::v1::PendingNexusOperationInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.last_attempt_failure {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.cancellation_info {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflow::v1::PostResetOperation {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(variant) = &mut self.variant {
            match variant {
                crate::protos::temporal::api::workflow::v1::post_reset_operation::Variant::SignalWorkflow(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflow::v1::post_reset_operation::SignalWorkflow {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflow.v1.PostResetOperation.SignalWorkflow.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflow::v1::WorkflowExecutionConfig {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.user_metadata {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflow::v1::WorkflowExecutionInfo {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.memo {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::CountActivityExecutionsResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.groups {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::count_activity_executions_response::AggregationGroup {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "temporal.api.workflowservice.v1.CountActivityExecutionsResponse.AggregationGroup.group_values",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.group_values),
        }).await?;
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::CountNexusOperationExecutionsResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.groups {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::count_nexus_operation_executions_response::AggregationGroup {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "temporal.api.workflowservice.v1.CountNexusOperationExecutionsResponse.AggregationGroup.group_values",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.group_values),
        }).await?;
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::CountSchedulesResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.groups {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::count_schedules_response::AggregationGroup {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "temporal.api.workflowservice.v1.CountSchedulesResponse.AggregationGroup.group_values",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.group_values),
        }).await?;
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::CountWorkflowExecutionsResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.groups {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::count_workflow_executions_response::AggregationGroup {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        visitor.visit(crate::payload_visitor::PayloadField {
            path: "temporal.api.workflowservice.v1.CountWorkflowExecutionsResponse.AggregationGroup.group_values",
            data: crate::payload_visitor::PayloadFieldData::Repeated(&mut self.group_values),
        }).await?;
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::CreateScheduleRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.schedule {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.memo {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::CreateWorkerDeploymentVersionRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.compute_config {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::DescribeActivityExecutionResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.info {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.DescribeActivityExecutionResponse.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.outcome {
            msg.visit_payloads_mut(visitor).await?;
        }
        for item in &mut self.callbacks {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::DescribeDeploymentResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.deployment_info {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::DescribeNexusOperationExecutionResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.info {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(payload) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.DescribeNexusOperationExecutionResponse.input",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
        for item in &mut self.completion_callbacks {
            item.visit_payloads_mut(visitor).await?;
        }
        if let Some(outcome) = &mut self.outcome {
            match outcome {
                crate::protos::temporal::api::workflowservice::v1::describe_nexus_operation_execution_response::Outcome::Result(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::workflowservice::v1::describe_nexus_operation_execution_response::Outcome::Failure(msg) => msg.visit_payloads_mut(visitor).await?,
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::DescribeScheduleResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.schedule {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.memo {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::DescribeWorkerDeploymentVersionResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.worker_deployment_version_info {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::DescribeWorkflowExecutionResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.execution_config {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.workflow_execution_info {
            msg.visit_payloads_mut(visitor).await?;
        }
        for item in &mut self.pending_activities {
            item.visit_payloads_mut(visitor).await?;
        }
        for item in &mut self.callbacks {
            item.visit_payloads_mut(visitor).await?;
        }
        for item in &mut self.pending_nexus_operations {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::ExecuteMultiOperationRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.operations {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::execute_multi_operation_request::Operation {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(operation) = &mut self.operation {
            match operation {
                crate::protos::temporal::api::workflowservice::v1::execute_multi_operation_request::operation::Operation::StartWorkflow(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::workflowservice::v1::execute_multi_operation_request::operation::Operation::UpdateWorkflow(msg) => msg.visit_payloads_mut(visitor).await?,
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::ExecuteMultiOperationResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.responses {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::execute_multi_operation_response::Response {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(response) = &mut self.response {
            match response {
                crate::protos::temporal::api::workflowservice::v1::execute_multi_operation_response::response::Response::StartWorkflow(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::workflowservice::v1::execute_multi_operation_response::response::Response::UpdateWorkflow(msg) => msg.visit_payloads_mut(visitor).await?,
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::GetCurrentDeploymentResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.current_deployment_info {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::GetDeploymentReachabilityResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.deployment_info {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::GetWorkflowExecutionHistoryResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.history {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::GetWorkflowExecutionHistoryReverseResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.history {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::ListActivityExecutionsResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.executions {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::ListArchivedWorkflowExecutionsResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.executions {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::ListClosedWorkflowExecutionsResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.executions {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::ListNexusOperationExecutionsResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.operations {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::ListOpenWorkflowExecutionsResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.executions {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::ListSchedulesResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.schedules {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::ListWorkflowExecutionsResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.executions {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::PollActivityExecutionResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.outcome {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::PollActivityTaskQueueResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.PollActivityTaskQueueResponse.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(payloads) = &mut self.heartbeat_details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.PollActivityTaskQueueResponse.heartbeat_details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::PollNexusOperationExecutionResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(outcome) = &mut self.outcome {
            match outcome {
                crate::protos::temporal::api::workflowservice::v1::poll_nexus_operation_execution_response::Outcome::Result(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::workflowservice::v1::poll_nexus_operation_execution_response::Outcome::Failure(msg) => msg.visit_payloads_mut(visitor).await?,
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::PollNexusTaskQueueResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.request {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::PollWorkflowExecutionUpdateResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.outcome {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::PollWorkflowTaskQueueResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.history {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.query {
            msg.visit_payloads_mut(visitor).await?;
        }
        for item in self.queries.values_mut() {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::QueryWorkflowRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.query {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::QueryWorkflowResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.query_result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.QueryWorkflowResponse.query_result",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RecordActivityTaskHeartbeatByIdRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.RecordActivityTaskHeartbeatByIdRequest.details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RecordActivityTaskHeartbeatRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.RecordActivityTaskHeartbeatRequest.details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::ResetWorkflowExecutionRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.post_reset_operations {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskCanceledByIdRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.RespondActivityTaskCanceledByIdRequest.details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskCanceledRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.RespondActivityTaskCanceledRequest.details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskCompletedByIdRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.RespondActivityTaskCompletedByIdRequest.result",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskCompletedRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.RespondActivityTaskCompletedRequest.result",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskFailedByIdRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(payloads) = &mut self.last_heartbeat_details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.RespondActivityTaskFailedByIdRequest.last_heartbeat_details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskFailedByIdResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.failures {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskFailedRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(payloads) = &mut self.last_heartbeat_details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.RespondActivityTaskFailedRequest.last_heartbeat_details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RespondActivityTaskFailedResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.failures {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RespondNexusTaskCompletedRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.response {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RespondNexusTaskFailedRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RespondQueryTaskCompletedRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.query_result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.RespondQueryTaskCompletedRequest.query_result",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RespondWorkflowTaskCompletedRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.commands {
            item.visit_payloads_mut(visitor).await?;
        }
        for item in self.query_results.values_mut() {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RespondWorkflowTaskCompletedResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.workflow_task {
            msg.visit_payloads_mut(visitor).await?;
        }
        for item in &mut self.activity_tasks {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::RespondWorkflowTaskFailedRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.failure {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::ScanWorkflowExecutionsResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in &mut self.executions {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::SetCurrentDeploymentRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.update_metadata {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::SetCurrentDeploymentResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.current_deployment_info {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.previous_deployment_info {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::SignalWithStartWorkflowExecutionRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(payloads) = &mut self.signal_input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest.signal_input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.memo {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.user_metadata {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::SignalWorkflowExecutionRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.SignalWorkflowExecutionRequest.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::StartActivityExecutionRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.StartActivityExecutionRequest.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.user_metadata {
            msg.visit_payloads_mut(visitor).await?;
        }
        for item in &mut self.completion_callbacks {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::StartBatchOperationRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(operation) = &mut self.operation {
            match operation {
                crate::protos::temporal::api::workflowservice::v1::start_batch_operation_request::Operation::TerminationOperation(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::workflowservice::v1::start_batch_operation_request::Operation::SignalOperation(msg) => msg.visit_payloads_mut(visitor).await?,
                crate::protos::temporal::api::workflowservice::v1::start_batch_operation_request::Operation::ResetOperation(msg) => msg.visit_payloads_mut(visitor).await?,
                _ => {}
            }
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::StartNexusOperationExecutionRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payload) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.StartNexusOperationExecutionRequest.input",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.user_metadata {
            msg.visit_payloads_mut(visitor).await?;
        }
        for item in &mut self.completion_callbacks {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::StartWorkflowExecutionRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.input {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.StartWorkflowExecutionRequest.input",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        if let Some(msg) = &mut self.memo {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.header {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.continued_failure {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(payloads) = &mut self.last_completion_result {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.StartWorkflowExecutionRequest.last_completion_result",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
        for item in &mut self.completion_callbacks {
            item.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.user_metadata {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::StartWorkflowExecutionResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.eager_workflow_task {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::TerminateWorkflowExecutionRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(payloads) = &mut self.details {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.TerminateWorkflowExecutionRequest.details",
                data: crate::payload_visitor::PayloadFieldData::Payloads(payloads),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::UpdateScheduleRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.schedule {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.search_attributes {
            msg.visit_payloads_mut(visitor).await?;
        }
        if let Some(msg) = &mut self.memo {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::UpdateWorkerDeploymentVersionComputeConfigRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in self.compute_config_scaling_groups.values_mut() {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::UpdateWorkerDeploymentVersionMetadataRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for payload in self.upsert_entries.values_mut() {
            visitor.visit(crate::payload_visitor::PayloadField {
                path: "temporal.api.workflowservice.v1.UpdateWorkerDeploymentVersionMetadataRequest.upsert_entries",
                data: crate::payload_visitor::PayloadFieldData::Single(payload),
            }).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::UpdateWorkerDeploymentVersionMetadataResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.metadata {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::UpdateWorkflowExecutionRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.request {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::UpdateWorkflowExecutionResponse {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        if let Some(msg) = &mut self.outcome {
            msg.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

#[allow(deprecated, clippy::single_match, clippy::collapsible_match)]
impl crate::payload_visitor::PayloadVisitable for crate::protos::temporal::api::workflowservice::v1::ValidateWorkerDeploymentVersionComputeConfigRequest {
    fn visit_payloads_mut<'a>(
        &'a mut self,
        visitor: &'a mut (dyn crate::payload_visitor::AsyncPayloadVisitor + Send),
    ) -> futures::future::BoxFuture<'a, Result<(), crate::data_converters::PayloadConversionError>> {
        Box::pin(async move {
        for item in self.compute_config_scaling_groups.values_mut() {
            item.visit_payloads_mut(visitor).await?;
        }
            Ok(())
        })
    }
}

