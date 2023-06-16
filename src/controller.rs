use std::convert::Into;
use std::env;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures::StreamExt;
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::core::v1::Pod;
use kube::runtime::reflector::ObjectRef;
use kube::{
    api::{Api, ListParams},
    client::Client,
    runtime::{
        controller::Controller,
        events::{Recorder, Reporter},
        watcher::Config,
    },
    Resource, ResourceExt,
};
use log::error;
use serde::Serialize;
use tokio::sync::RwLock;

use crate::deployments;

pub enum ControllerType {
    Pod,
    Deployment,
    ReplicaSet,
    DaemonSet,
    StatefulSet,
}

// Context for our reconciler
#[derive(Clone)]
pub struct Context {
    /// Kubernetes client
    pub client: Client,
    /// Diagnostics read by the web server
    pub diagnostics: Arc<RwLock<Diagnostics>>,
}

/// Diagnostics to be exposed by the web server
#[derive(Clone, Serialize)]
pub struct Diagnostics {
    #[serde(deserialize_with = "from_ts")]
    pub last_event: DateTime<Utc>,
    #[serde(skip)]
    pub reporter: Reporter,
}
impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            last_event: Utc::now(),
            reporter: "annotations-controller".into(),
        }
    }
}
impl Diagnostics {
    pub fn recorder(&self, client: Client, deployment: &Deployment) -> Recorder {
        Recorder::new(client, self.reporter.clone(), deployment.object_ref(&()))
    }
}

/// State shared between the controller and the web server
#[derive(Clone, Default)]
pub struct State {
    /// Diagnostics populated by the reconciler
    diagnostics: Arc<RwLock<Diagnostics>>,
}

/// State wrapper around the controller outputs for the web server
impl State {
    /// State getter
    pub async fn diagnostics(&self) -> Diagnostics {
        self.diagnostics.read().await.clone()
    }

    // Create a Controller Context that can update State
    pub fn to_context(&self, client: Client) -> Arc<Context> {
        Arc::new(Context {
            client,
            diagnostics: self.diagnostics.clone(),
        })
    }
}

/// Initialize the controller and shared state
pub async fn run(state: State) {
    let client = Client::try_default()
        .await
        .expect("failed to create kube Client");
    let namespace = env::var("NAMESPACE").map(Some).unwrap_or(None);
    let deployments: Api<Deployment> = match namespace {
        Some(n) => Api::namespaced(client.clone(), &n),
        None => Api::all(client.clone()),
    };

    if let Err(e) = deployments.list(&ListParams::default().limit(1)).await {
        error!("Deployments are not queryable; {e:?}.");
        std::process::exit(1);
    }
    let config = Config::default().any_semantic();
    Controller::new(deployments, config)
        .shutdown_on_signal()
        .run(
            deployments::reconcile,
            deployments::error_policy,
            state.to_context(client),
        )
        .filter_map(|x| async move { std::result::Result::ok(x) })
        .for_each(|_| futures::future::ready(()))
        .await;
}

/*
pub async fn run_type(typ: ControllerType, state: State) {
    let client = Client::try_default()
        .await
        .expect("failed to create kube Client");
    let namespace = env::var("NAMESPACE").map(Some).unwrap_or(None);

    let config = Config::default().labels(ENABLED);
    match typ {
        ControllerType::Deployment => {
            let deployments: Api<Deployment> = match namespace {
                Some(n) => Api::namespaced(client.clone(), &n),
                None => Api::all(client.clone()),
            };

            if let Err(e) = deployments.list(&ListParams::default().limit(1)).await {
                error!("Deployments are not queryable; {e:?}.");
                std::process::exit(1);
            }
            Controller::new(deployments, config)
                .shutdown_on_signal()
                .run(
                    deployments::reconcile,
                    deployments::error_policy,
                    state.to_context(client),
                )
                .filter_map(|x| async move { std::result::Result::ok(x) })
                .for_each(|_| futures::future::ready(()))
                .await;
        }
        _ => {}
    }
}
*/
