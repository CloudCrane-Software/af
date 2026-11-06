//! smoke starter：启动工作流并等待结果
use rust_sdk_smoke::{SmokeWorkflow, TASK_QUEUE};
use temporalio_client::{
    Client, ClientOptions, Connection, WorkflowGetResultOptions, WorkflowStartOptions,
    envconfig::LoadClientConfigProfileOptions,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (conn_opts, client_opts) =
        ClientOptions::load_from_config(LoadClientConfigProfileOptions::default())?;
    let connection = Connection::connect(conn_opts).await?;
    let client = Client::new(connection, client_opts)?;

    let handle = client
        .start_workflow(
            SmokeWorkflow::run,
            21_i64,
            WorkflowStartOptions::new(TASK_QUEUE, "rust-smoke-wf-1").build(),
        )
        .await?;
    println!("[starter] started, run_id={:?}", handle.run_id());

    let result = handle.get_result(WorkflowGetResultOptions::default()).await?;
    println!("[starter] workflow result = {result} (expect 84)");
    Ok(())
}
