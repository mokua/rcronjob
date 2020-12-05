#[macro_use]
extern crate log;

use color_eyre::{Report, Result};
use futures::StreamExt;
use k8s_openapi::api::core::v1::ObjectReference;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{Time, ObjectMeta, OwnerReference};
use kube::{Api, Client};
//use k8s_openapi::api::batch::v1::CronJob;
use kube::api::{Meta, PostParams, ListParams};
use kube_derive::CustomResource;
use serde::{Deserialize, Serialize};
use kube_runtime::Controller;
use kube_runtime::controller::{Context, ReconcilerAction};
use snafu::{Backtrace, OptionExt, ResultExt, Snafu};
use tokio::time::Duration;
use std::collections::BTreeMap;
//use crate::Error::MissingObjectKey;

#[derive(CustomResource, Serialize, Deserialize, Debug, PartialEq, Clone)]
#[kube(group = "batch.tutorial.mokualabs.com", version = "v1", kind = "CronJob", namespaced, shortname = "cronJob")]
#[kube(status = "CronJobStatus")]
#[kube(printcolumn = r#"{"name":"Schedule", "type":"string", "description":"the schedule", "jsonPath":".spec.schedule"}"#)]
#[kube(printcolumn = r#"{"name":"Starting Deadline Seconds", "type":"integer", "description":"deadline", "jsonPath":".spec.startingDeadlineSeconds"}"#)]
pub struct CronJobSpec {
    pub schedule: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starting_deadline_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency_policy: Option<ConcurrencyPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspend: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub successful_jobs_history_limit: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_jobs_history_limit: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct CronJobStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<Vec<ObjectReference>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_schedule_time: Option<Time>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub enum ConcurrencyPolicy {
    Allow,
    Forbid,
    Replace,
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Failed to create ConfigMap: {}", source))]
    ConfigMapCreationFailed {
        source: kube::Error,
       // backtrace: Backtrace,
    },
    MissingObjectKey {
        name: &'static str,
        //backtrace: Backtrace,
    },
    SerializationFailed {
        source: serde_json::Error,
        //backtrace: Backtrace,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    use k8s_openapi::Resource;
    println!("Hello, world!");

    std::env::set_var("RUST_LOG", "info,kube=debug");
    env_logger::init();
    let client = Client::try_default().await?;
    let namespace = std::env::var("NAMESPACE").unwrap_or("default".into());
    // Create the CRD so we can create Foos in kube
    let cron_job_crd = CronJob::crd();
    info!("Creating cron job CRD: {}", serde_yaml::to_string(&cron_job_crd)?);

    // Manage the Foo CR
    let cron_job_api: Api<CronJob> = Api::namespaced(client.clone(), &namespace);
    let pp = PostParams::default();

    // Create CronJob cronJob
    info!("Creating CronJob instance cronJob");
    let f1 = CronJob::new("cronjob-sample-5", CronJobSpec {
        schedule: "*/1 * * * *".to_string(),
        starting_deadline_seconds: Some(60),
        concurrency_policy: Some(ConcurrencyPolicy::Allow),
        suspend: Some(false),
        successful_jobs_history_limit: Some(4),
        failed_jobs_history_limit: Some(6),
    });
    let o = cron_job_api.create(&pp, &f1).await?;
    assert_eq!(Meta::name(&f1), Meta::name(&o));
    info!("Created {}", Meta::name(&o));

    // Verify we can get it
    info!("Get cron job instance");
    let f1cpy = cron_job_api.get("cronjob-sample-5").await?;
    //assert_eq!(f1cpy.spec.info, "old baz");
    println!("{:?}", f1cpy);

    Controller::new(cron_job_api, ListParams::default())
        //.owns(cms, ListParams::default())
        .run(reconcile, error_policy, Context::new(Data { client }))
        .for_each(|res| async move {
            match res {
                Ok(o) => info!("reconciled {:?}", o),
                Err(e) => warn!("reconcile failed: {}", Report::from(e)),
            }
        })
        .await;

    Ok(())
}

/// Controller triggers this whenever our main object or our children changed
async fn reconcile(generator: CronJob, ctx: Context<Data>) -> Result<ReconcilerAction, Error> {
    let client = ctx.get_ref().client.clone();
    info!("in reconcile .. ");
    //fetch k8s api CronJob
    let cj_api = Api::<CronJob>::namespaced(
        client.clone(),
        generator.metadata.namespace.as_ref().context(MissingObjectKey {
            name: ".metadata.namespace",
        })?,
    );

    let lp = ListParams::default();
       // .fields("jobOwnerKey = req.Name" ); // for this app only
    for p in cj_api.list(&lp).await {
        for item in p.items {
            info!("Found cron job: {}", Meta::name(&item));

        }

    }

    Ok(ReconcilerAction {
        requeue_after: Some(Duration::from_secs(10)),
    })
}


fn object_to_owner_reference<K: Meta>(meta: ObjectMeta) -> Result<OwnerReference, Error> {
    Ok(OwnerReference {
        api_version: K::API_VERSION.to_string(),
        kind: K::KIND.to_string(),
        name: meta.name.context(MissingObjectKey {
            name: ".metadata.name",
        })?,
        uid: meta.uid.context(MissingObjectKey {
            name: ".metadata.backtrace",
        })?,
        ..OwnerReference::default()
    })
}

// Data we want access to in error/reconcile calls
struct Data {
    client: Client,
}

/// The controller triggers this on reconcile errors
fn error_policy(_error: &Error, _ctx: Context<Data>) -> ReconcilerAction {
    ReconcilerAction {
        requeue_after: Some(Duration::from_secs(1)),
    }
}