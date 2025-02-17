// Copyright Materialize, Inc. and contributors. All rights reserved.
//
// Use of this software is governed by the Business Source License
// included in the LICENSE file.
//
// As of the Change Date specified in that file, in accordance with
// the Business Source License, use of this software will be governed
// by the Apache License, Version 2.0.

use std::collections::HashMap;
use std::fs;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use async_trait::async_trait;
use itertools::Itertools;
use scopeguard::defer;
use tokio::process::Command;
use tokio::task::JoinHandle;
use tokio::time::{self, Duration};
use tracing::{error, info};

use mz_orchestrator::{NamespacedOrchestrator, Orchestrator, Service, ServiceConfig};
use mz_ore::id_gen::IdAllocator;

/// Configures a [`ProcessOrchestrator`].
#[derive(Debug, Clone)]
pub struct ProcessOrchestratorConfig {
    /// The directory in which the orchestrator should look for executable
    /// images.
    pub image_dir: PathBuf,
    /// The range of ports to allocate.
    pub port_range: RangeInclusive<i32>,
}

/// An orchestrator backed by processes on the local machine.
///
/// **This orchestrator is for development only.** Due to limitations in the
/// Unix process API, it does not exactly conform to the documented semantics
/// of `Orchestrator`.
#[derive(Debug, Clone)]
pub struct ProcessOrchestrator {
    image_dir: PathBuf,
    port_allocator: Arc<IdAllocator<i32>>,
}

impl ProcessOrchestrator {
    /// Creates a new process orchestrator from the provided configuration.
    pub async fn new(
        ProcessOrchestratorConfig {
            image_dir,
            port_range,
        }: ProcessOrchestratorConfig,
    ) -> Result<ProcessOrchestrator, anyhow::Error> {
        Ok(ProcessOrchestrator {
            image_dir: fs::canonicalize(image_dir)?,
            port_allocator: Arc::new(IdAllocator::new(*port_range.start(), *port_range.end())),
        })
    }
}

impl Orchestrator for ProcessOrchestrator {
    fn namespace(&self, namespace: &str) -> Box<dyn NamespacedOrchestrator> {
        Box::new(NamespacedProcessOrchestrator {
            namespace: namespace.into(),
            image_dir: self.image_dir.clone(),
            port_allocator: Arc::clone(&self.port_allocator),
            supervisors: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

#[derive(Debug, Clone)]
struct NamespacedProcessOrchestrator {
    namespace: String,
    image_dir: PathBuf,
    port_allocator: Arc<IdAllocator<i32>>,
    supervisors: Arc<Mutex<HashMap<String, Vec<JoinHandle<()>>>>>,
}

#[async_trait]
impl NamespacedOrchestrator for NamespacedProcessOrchestrator {
    async fn ensure_service(
        &mut self,
        id: &str,
        ServiceConfig {
            image,
            args,
            ports: ports_in,
            memory_limit: _,
            cpu_limit: _,
            processes: processes_in,
            labels: _,
        }: ServiceConfig<'_>,
    ) -> Result<Box<dyn Service>, anyhow::Error> {
        let full_id = format!("{}-{}", self.namespace, id);
        let mut supervisors = self.supervisors.lock().expect("lock poisoned");
        if supervisors.contains_key(id) {
            unimplemented!("ProcessOrchestrator does not yet support updating existing services");
        }
        let path = self.image_dir.join(image);
        let mut processes = vec![];
        let mut handles = vec![];
        for _ in 0..processes_in {
            let mut ports = HashMap::new();
            for port in &ports_in {
                let p = self
                    .port_allocator
                    .alloc()
                    .ok_or_else(|| anyhow!("port exhaustion"))?;
                ports.insert(port.name.clone(), p);
            }
            let args = args(&ports);
            processes.push(ports.clone());
            handles.push(mz_ore::task::spawn(
                || format!("service-supervisor: {full_id}"),
                {
                    let full_id = full_id.clone();
                    let args = args.clone();
                    let path = path.clone();
                    let port_allocator = Arc::clone(&self.port_allocator);
                    async move {
                        defer! {
                            for port in ports.values() {
                                port_allocator.free(*port);
                            }
                        }
                        loop {
                            info!(
                                "Launching {}: {} {}...",
                                full_id,
                                path.display(),
                                args.iter().join(" ")
                            );
                            match Command::new(&path).args(&args).status().await {
                                Ok(status) => {
                                    error!("{} exited: {}; relaunching in 5s", full_id, status);
                                }
                                Err(e) => {
                                    error!(
                                        "{} failed to launch: {}; relaunching in 5s",
                                        full_id, e
                                    );
                                }
                            }
                            time::sleep(Duration::from_secs(5)).await;
                        }
                    }
                },
            ))
        }
        supervisors.insert(id.into(), handles);
        Ok(Box::new(ProcessService { processes }))
    }

    async fn drop_service(&mut self, id: &str) -> Result<(), anyhow::Error> {
        let mut supervisors = self.supervisors.lock().expect("lock poisoned");
        if let Some(handles) = supervisors.remove(id) {
            for handle in handles {
                handle.abort();
            }
        }
        Ok(())
    }

    async fn list_services(&self) -> Result<Vec<String>, anyhow::Error> {
        let supervisors = self.supervisors.lock().expect("lock poisoned");
        Ok(supervisors.keys().cloned().collect())
    }
}

#[derive(Debug, Clone)]
struct ProcessService {
    /// For each process in order, the allocated ports by name.
    processes: Vec<HashMap<String, i32>>,
}

impl Service for ProcessService {
    fn addresses(&self, port: &str) -> Vec<String> {
        self.processes
            .iter()
            .map(|p| format!("localhost:{}", p[port]))
            .collect()
    }
}
