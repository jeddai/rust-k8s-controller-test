use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use k8s_openapi::api::apps::v1::Deployment;
use kube::api::{Patch, PatchParams};
use kube::runtime::controller::Action;
use kube::runtime::events::{Event, EventType};
use kube::runtime::finalizer::{finalizer, Event as Finalizer};
use kube::{Api, Resource, ResourceExt};
use log::{info, warn};
use serde_json::json;

use crate::constants::{CirrusEnvironment, FINALIZER};
use crate::{constants::ANNOTATION_PREFIX, Context, Error, Result};

fn create_env(deployment: Arc<Deployment>) -> Result<CirrusEnvironment> {
    let mut env: CirrusEnvironment = Default::default();
    let nimbus_annotations = deployment
        .annotations()
        .iter()
        .filter_map(|(k, v)| {
            if k.starts_with(ANNOTATION_PREFIX) {
                Some((k.replace(ANNOTATION_PREFIX, ""), v.clone()))
            } else {
                None
            }
        })
        .collect::<Vec<(String, String)>>();

    for (name, value) in nimbus_annotations {
        if name.starts_with("env.") {
            let new = name.replace("env.", "").replace('.', "_").to_uppercase();
            env.set(new, value);
        }
    }

    env.validate(&deployment.name_any())?;
    Ok(env)
}

async fn reconcile_deployment(ctx: Arc<Context>, deployment: Arc<Deployment>) -> Result<Action> {
    let deployment_ref = &deployment.clone();

    let client = ctx.client.clone();
    let recorder = ctx
        .diagnostics
        .read()
        .await
        .recorder(client.clone(), deployment_ref);
    let ns = deployment_ref.namespace().unwrap();
    let name = deployment_ref.name_any();
    let deployments: Api<Deployment> = Api::namespaced(client, &ns);

    let env = create_env(deployment)?;

    info!("Deployment {} has enabled Nimbus", &name);
    recorder
        .publish(Event {
            type_: EventType::Normal,
            reason: "NimbusEnabled".into(),
            note: Some(format!("Enabled Nimbus for `{name}`")),
            action: "Enabling Nimbus".into(),
            secondary: None,
        })
        .await
        .map_err(Error::KubeError)?;

    let patch = Patch::Strategic(json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "spec": {
            "template": {
                "metadata": {
                    "labels": {
                        "nimbus": "enabled"
                    }
                },
                "spec": {
                    "containers": [{
                        "name": "cirrus",
                        "image": "experimenter-cirrus",
                        "imagePullPolicy": "IfNotPresent",
                        "ports": [{
                            "containerPort": 8001,
                            "name": "cirrus"
                        }],
                        "env": env.into_iter().map(|(k, v)| {
                            serde_json::map::Map::from_iter([
                                ("name".into(), serde_json::Value::String(k)),
                                ("value".into(), serde_json::Value::String(v)),
                            ])
                        }).collect::<Vec<serde_json::map::Map<String, serde_json::Value>>>(),
                        "volumeMounts": [{
                            "name": "cirrus-glean",
                            "mountPath": "/glean"
                        }]
                    }],
                    "volumes": [{
                        "name": "cirrus-glean",
                        "emptyDir": {
                            "sizeLimit": "2Mi"
                        }
                    }]
                }
            }
        }
    }));
    let ps = PatchParams::apply("nimbus-controller");
    let _o = deployments
        .patch(&name, &ps, &patch)
        .await
        .map_err(Error::KubeError)?;

    info!("Deployment {} patched", name);

    Ok(Action::requeue(Duration::from_secs(5 * 60)))
}

async fn cleanup_deployment(ctx: Arc<Context>, deployment: Arc<Deployment>) -> Result<Action> {
    let recorder = ctx
        .diagnostics
        .read()
        .await
        .recorder(ctx.client.clone(), &deployment.clone());
    recorder
        .publish(Event {
            type_: EventType::Normal,
            reason: "DeleteRequested".into(),
            note: Some(format!("Delete `{}`", deployment.name_any())),
            action: "Deleting".into(),
            secondary: None,
        })
        .await
        .map_err(Error::KubeError)?;
    info!("Cleaned up Deployment {}", deployment.name_any());
    Ok(Action::await_change())
}

pub async fn reconcile(deployment: Arc<Deployment>, ctx: Arc<Context>) -> Result<Action> {
    ctx.diagnostics.write().await.last_event = Utc::now();
    let ns = deployment.namespace().unwrap(); // deployment is namespace scoped
    let deployments: Api<Deployment> = Api::namespaced(ctx.client.clone(), &ns);

    if deployment
        .annotations()
        .get("nimbus.mozilla.org/enabled")
        .map(|v| v != "true")
        .unwrap_or(true)
    {
        info!(
            r#"Deployment {} does not have an annotation equivalent to "nimbus.mozilla.org/enabled='true'""#,
            deployment.name_any()
        );
        return Ok(Action::await_change());
    }

    let env = create_env(deployment.clone())?;
    let finalizer_index = deployment
        .finalizers()
        .iter()
        .enumerate()
        .find(|(_, fin)| *fin == FINALIZER)
        .map(|(i, _)| i);
    let cirrus_container_exists = deployment
        .spec
        .clone()
        .unwrap()
        .template
        .spec
        .unwrap()
        .containers
        .iter()
        .find(|c| c.name == "cirrus")
        .map(|c| match c.env.clone() {
            Some(env_vars) => {
                let mut env = CirrusEnvironment::default();
                for e in env_vars {
                    env.set(e.name, e.value.unwrap_or("".into()));
                }
                Some(env)
            }
            None => None,
        })
        .unwrap_or(None);
    if cirrus_container_exists.is_some()
        && cirrus_container_exists.unwrap() == env
        && finalizer_index.is_some()
        && deployment.meta().deletion_timestamp.is_none()
    {
        // TODO make this smarter — it needs to look deeper. Maybe concat all the environment variables and compare the container env to the current env?
        info!("Skipping this reconciliation — deployment {} is not being deleted, has been finalized, and the Cirrus container exists", deployment.name_any());
        return Ok(Action::requeue(Duration::from_secs(5 * 60)));
    }

    info!(
        "Reconciling Deployment \"{}\" in {}",
        deployment.name_any(),
        ns
    );
    finalizer(&deployments, FINALIZER, deployment, |event| async {
        match event {
            Finalizer::Apply(deployment) => reconcile_deployment(ctx.clone(), deployment).await,
            Finalizer::Cleanup(deployment) => cleanup_deployment(ctx.clone(), deployment).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
}

pub fn error_policy(_deployment: Arc<Deployment>, error: &Error, _ctx: Arc<Context>) -> Action {
    warn!("reconcile failed: {:?}", error);
    Action::requeue(Duration::from_secs(5 * 60))
}
