//! Rust SDK smoke：worker + starter 一体，对 dev server 跑一个"活动+计时器"工作流全往返。
use std::time::Duration;

use temporalio_client::{
    Client, ClientOptions, Connection, WorkflowGetResultOptions, WorkflowStartOptions,
    envconfig::LoadClientConfigProfileOptions,
};
use temporalio_macros::{activities, workflow, workflow_methods};
use temporalio_sdk::{
    ActivityOptions, Runtime, Worker, WorkerOptions, WorkflowContext, WorkflowResult,
    activities::{ActivityContext, ActivityError},
};

#[activities]
impl MathActivities {
    #[activity]
    pub async fn double(_ctx: ActivityContext, n: i64) -> Result<i64, ActivityError> {
        Ok(n * 2)
    }
}

pub struct MathActivities;

#[workflow]
#[derive(Default)]
pub struct SmokeWorkflow;

#[workflow_methods]
impl SmokeWorkflow {
    #[run]
    pub async fn run(ctx: &mut WorkflowContext<Self>, input: i64) -> WorkflowResult<i64> {
        let a = ctx
            .execute_activity(
                MathActivities::double,
                input,
                ActivityOptions::start_to_close_timeout(Duration::from_secs(10)),
            )
            .await?;
        ctx.timer(Duration::from_secs(1)).await; // 持久计时器也走一次
        let b = ctx
            .execute_activity(
                MathActivities::double,
                a,
                ActivityOptions::start_to_close_timeout(Duration::from_secs(10)),
            )
            .await?;
        Ok(b)
    }
}


pub const TASK_QUEUE: &str = "rust-smoke-queue";
