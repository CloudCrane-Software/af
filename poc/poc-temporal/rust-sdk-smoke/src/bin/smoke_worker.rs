//! smoke worker：跑在 rust-smoke-queue 上
use rust_sdk_smoke::{MathActivities, SmokeWorkflow, TASK_QUEUE};
use temporalio_client::{
    Client, ClientOptions, Connection, envconfig::LoadClientConfigProfileOptions,
};
use temporalio_sdk::{Runtime, Worker, WorkerOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = Runtime::from_current_tokio(Default::default())?;
    let (conn_opts, client_opts) =
        ClientOptions::load_from_config(LoadClientConfigProfileOptions::default())?;
    let connection = Connection::connect(conn_opts).await?;
    let client = Client::new(connection, client_opts)?;
    println!("[worker] connected");

    let worker_options = WorkerOptions::new(TASK_QUEUE)
        .register_workflow::<SmokeWorkflow>()?
        .register_activities(MathActivities)
        .build();
    let mut worker = Worker::new(&runtime, client, worker_options)?;
    println!("[worker] up on {TASK_QUEUE}");
    worker.run().await?;
    Ok(())
}
