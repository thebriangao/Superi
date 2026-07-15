use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::{CapabilityId, CapabilitySet, SemanticVersion};
use superi_effects::authoring::{EffectInstanceBindings, EffectParameterBinding};
use superi_effects::ofx::{
    IsolatedOfxAdapter, OfxAdapterContract, OfxAdapterIsolation, OfxClipDescriptor,
    OfxClipDirection, OfxContext, OfxContextDescriptor, OfxCreateInstanceRequest, OfxImageResource,
    OfxInstanceKey, OfxLoadRequest, OfxParameterDescriptor, OfxParameterKind, OfxParameterValue,
    OfxPluginCapabilities, OfxPluginDescriptor, OfxPluginIdentity, OfxPluginLifecycle,
    OfxRenderReceipt, OfxRenderRequest, OfxRenderThreadSafety, OfxRenderWindow, OfxTime,
};
use superi_engine::lifecycle::EngineWorkKind;
use superi_engine::plugins::{
    discover_ofx_bundles, OfxPluginBundle, OfxWorkerLauncher, PluginFailureAction, PluginSupervisor,
};
use superi_graph::expr::ParameterAddress;
use superi_graph::ids::{GraphId, NodeId, ParameterId, PortId};
use superi_graph::mutate::{
    EditableGraph, GraphMutation, GraphTransaction, InstancePort, TypedParameterValue,
};
use superi_graph::node::{ParameterName, PortName};
use superi_graph::value::GraphValue;

const WORKER_ENV: &str = "SUPERI_PLUGIN_SUPERVISION_WORKER";
const WORKER_COMMAND_ENV: &str = "SUPERI_PLUGIN_SUPERVISION_COMMAND";
const WORKER_PREFIX: &str = "SUPERI_WORKER|";

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "superi-plugin-supervision-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn bundle(&self, relative: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::create_dir_all(path.join("Contents")).unwrap();
        path
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct ProcessAdapter {
    plugin: OfxPluginDescriptor,
    context: OfxContextDescriptor,
    activation_attempts: u32,
    worker_generation: u32,
}

impl ProcessAdapter {
    fn new(requested_permissions: CapabilitySet) -> Result<Self> {
        let identity = OfxPluginIdentity::new(
            "com.example.superi-process-effect",
            SemanticVersion::new(1, 0, 0),
        )?;
        let plugin = OfxPluginDescriptor::new(
            identity,
            "Process Effect",
            "A real child-process OpenFX contract fixture.",
            [OfxContext::Filter],
            OfxPluginCapabilities::new(
                true,
                true,
                false,
                OfxRenderThreadSafety::InstanceSafe,
                false,
            ),
            requested_permissions,
        )?;
        Ok(Self {
            plugin,
            context: filter_context(),
            activation_attempts: 0,
            worker_generation: 1,
        })
    }

    fn round_trip(&mut self, command: &str) -> Result<()> {
        let child = Command::new(std::env::current_exe().map_err(worker_io_error)?)
            .arg("--exact")
            .arg("plugin_worker_process")
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env(WORKER_ENV, "1")
            .env(WORKER_COMMAND_ENV, command)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(worker_io_error)?;
        let worker_pid = child.id();
        assert_ne!(worker_pid, std::process::id());
        let output = child.wait_with_output().map_err(worker_io_error)?;
        if !output.status.success() {
            return Err(Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "plugin worker exited before completing its action",
            )
            .with_context(
                ErrorContext::new("superi-engine-test.plugin-worker", "round_trip")
                    .with_field("command", command)
                    .with_field("worker_pid", worker_pid.to_string())
                    .with_field("worker_generation", self.worker_generation.to_string()),
            ));
        }
        let stdout = String::from_utf8(output.stdout).map_err(|source| {
            Error::with_source(
                ErrorCategory::CorruptData,
                Recoverability::Terminal,
                "isolated plugin worker returned non-UTF-8 protocol output",
                source,
            )
        })?;
        let reply = stdout
            .lines()
            .find_map(|line| line.split_once(WORKER_PREFIX).map(|(_, reply)| reply))
            .ok_or_else(|| {
                Error::new(
                    ErrorCategory::CorruptData,
                    Recoverability::Terminal,
                    "isolated plugin worker omitted its protocol reply",
                )
                .with_context(
                    ErrorContext::new("superi-engine-test.plugin-worker", "round_trip")
                        .with_field("command", command)
                        .with_field("worker_pid", worker_pid.to_string()),
                )
            })?;
        match reply {
            "ok" => Ok(()),
            "retryable" => Err(Error::new(
                ErrorCategory::Timeout,
                Recoverability::Retryable,
                "isolated plugin worker exceeded its action deadline",
            )
            .with_context(
                ErrorContext::new("superi-engine-test.plugin-worker", "round_trip")
                    .with_field("command", command)
                    .with_field("worker_pid", worker_pid.to_string())
                    .with_field("worker_generation", self.worker_generation.to_string()),
            )),
            other => Err(Error::new(
                ErrorCategory::CorruptData,
                Recoverability::Terminal,
                "isolated plugin worker returned an invalid reply",
            )
            .with_context(
                ErrorContext::new("superi-engine-test.plugin-worker", "round_trip")
                    .with_field("command", command)
                    .with_field("reply", other),
            )),
        }
    }
}

impl IsolatedOfxAdapter for ProcessAdapter {
    fn contract(&self) -> OfxAdapterContract {
        OfxAdapterContract::new(
            OfxAdapterIsolation::WorkerProcess,
            1,
            4 * 1024 * 1024,
            30_000,
            true,
        )
    }

    fn load(&mut self, request: OfxLoadRequest) -> Result<()> {
        if request.is_scan() {
            assert!(request.granted_permissions().is_empty());
            return self.round_trip("load_scan");
        }

        let granted = request
            .granted_permissions()
            .iter()
            .map(CapabilityId::as_str)
            .collect::<Vec<_>>();
        if granted != ["superi.permission.media"] {
            return Err(Error::new(
                ErrorCategory::PermissionDenied,
                Recoverability::Terminal,
                "plugin worker received permissions beyond its exact request",
            )
            .with_context(
                ErrorContext::new("superi-engine-test.plugin-worker", "load")
                    .with_field("granted", granted.join(",")),
            ));
        }
        self.activation_attempts += 1;
        if self.activation_attempts == 1 {
            self.round_trip("load_activation_retryable")
        } else {
            self.round_trip("load_activation")
        }
    }

    fn describe(&mut self) -> Result<OfxPluginDescriptor> {
        self.round_trip("describe")?;
        Ok(self.plugin.clone())
    }

    fn describe_in_context(&mut self, context: OfxContext) -> Result<OfxContextDescriptor> {
        self.round_trip("describe_in_context")?;
        if context == OfxContext::Filter {
            Ok(self.context.clone())
        } else {
            Err(Error::new(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "test worker does not expose the requested OpenFX context",
            ))
        }
    }

    fn create_instance(&mut self, _request: OfxCreateInstanceRequest) -> Result<()> {
        self.round_trip("create_instance")
    }

    fn render(&mut self, request: OfxRenderRequest) -> Result<OfxRenderReceipt> {
        self.round_trip("render_retryable")?;
        OfxRenderReceipt::new(
            request
                .image("Output")
                .expect("host validates the output binding")
                .resource_token(),
        )
    }

    fn destroy_instance(&mut self, _key: OfxInstanceKey) -> Result<()> {
        self.round_trip("destroy_instance")
    }

    fn unload(&mut self) -> Result<()> {
        self.round_trip("unload")
    }

    fn restart_worker(&mut self) -> Result<()> {
        self.worker_generation += 1;
        self.round_trip("restart")
    }
}

struct ProcessLauncher {
    requested_permissions: CapabilitySet,
}

impl OfxWorkerLauncher for ProcessLauncher {
    fn launch(&mut self, bundle: &OfxPluginBundle) -> Result<Box<dyn IsolatedOfxAdapter>> {
        if bundle.name() == "broken.ofx.bundle" {
            return Err(Error::new(
                ErrorCategory::CorruptData,
                Recoverability::Terminal,
                "plugin bundle failed platform signature validation",
            )
            .with_context(
                ErrorContext::new("superi-engine-test.plugin-launcher", "launch")
                    .with_field("bundle", bundle.path().display().to_string()),
            ));
        }
        Ok(Box::new(ProcessAdapter::new(
            self.requested_permissions.clone(),
        )?))
    }
}

fn worker_io_error(source: std::io::Error) -> Error {
    Error::with_source(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "test plugin worker I/O failed",
        source,
    )
}

fn filter_context() -> OfxContextDescriptor {
    OfxContextDescriptor::new(
        OfxContext::Filter,
        [
            OfxClipDescriptor::new(
                "Source",
                "Source",
                "Required input image.",
                OfxClipDirection::Input,
                false,
            )
            .unwrap(),
            OfxClipDescriptor::new(
                "Output",
                "Output",
                "Rendered output image.",
                OfxClipDirection::Output,
                false,
            )
            .unwrap(),
        ],
        [OfxParameterDescriptor::new(
            "Gain",
            "Gain",
            "Editable gain parameter.",
            OfxParameterKind::Double,
            OfxParameterValue::double(1.0).unwrap(),
            true,
            true,
            Vec::<String>::new(),
        )
        .unwrap()],
    )
    .unwrap()
}

fn bindings() -> EffectInstanceBindings {
    EffectInstanceBindings::new(
        [InstancePort::new(
            PortId::from_raw(10),
            PortName::new("source").unwrap(),
        )],
        [InstancePort::new(
            PortId::from_raw(11),
            PortName::new("output").unwrap(),
        )],
        [EffectParameterBinding::new(
            ParameterId::from_raw(20),
            ParameterName::new("gain").unwrap(),
        )],
    )
}

#[test]
fn plugin_worker_process() {
    if std::env::var_os(WORKER_ENV).is_none() {
        return;
    }
    let command = std::env::var(WORKER_COMMAND_ENV).unwrap();
    let reply = match command.as_str() {
        "load_activation_retryable" | "render_retryable" => "retryable",
        "load_scan"
        | "load_activation"
        | "describe"
        | "describe_in_context"
        | "create_instance"
        | "destroy_instance"
        | "unload"
        | "restart" => "ok",
        _ => "invalid",
    };
    println!("{WORKER_PREFIX}{reply}");
}

#[test]
fn discovery_supervision_and_workflows_share_one_contained_plugin_state() {
    let tree = TempTree::new();
    tree.bundle("z/healthy.ofx.bundle");
    tree.bundle("a/broken.ofx.bundle");
    tree.bundle("@ignored/hidden.ofx.bundle");
    fs::create_dir_all(tree.root().join("malformed.ofx.bundle")).unwrap();

    let discovery = discover_ofx_bundles([tree.root().to_path_buf()]);
    assert_eq!(
        discovery
            .bundles()
            .iter()
            .map(OfxPluginBundle::name)
            .collect::<Vec<_>>(),
        ["broken.ofx.bundle", "healthy.ofx.bundle"]
    );
    assert_eq!(discovery.failures().len(), 1);
    assert_eq!(
        discovery.failures()[0].recoverability(),
        Recoverability::UserCorrectable
    );
    assert_eq!(
        discovery.failures()[0].recommended_action(),
        PluginFailureAction::CorrectConfiguration
    );

    let media = CapabilityId::new("superi.permission.media").unwrap();
    let network = CapabilityId::new("superi.permission.network").unwrap();
    let requested = CapabilitySet::new([media.clone()]);
    let mut launcher = ProcessLauncher {
        requested_permissions: requested.clone(),
    };
    let mut supervisor = PluginSupervisor::scan(discovery, &mut launcher, 2).unwrap();

    assert_eq!(supervisor.plugin_count(), 1);
    assert_eq!(supervisor.discovered_registry().len(), 1);
    assert!(supervisor.active_registry().is_empty());
    assert!(supervisor.failures().iter().any(|failure| {
        failure.recoverability() == Recoverability::Terminal
            && failure.recommended_action() == PluginFailureAction::Stop
            && failure.message().contains("signature validation")
    }));

    let identity = OfxPluginIdentity::new(
        "com.example.superi-process-effect",
        SemanticVersion::new(1, 0, 0),
    )
    .unwrap();
    let definition = supervisor
        .definition::<()>(&identity, OfxContext::Filter)
        .unwrap();
    let node = definition
        .instantiate(bindings(), std::iter::empty())
        .unwrap();
    let node_id = NodeId::from_raw(100);
    let mut graph = EditableGraph::new(GraphId::from_raw(50));
    let snapshot = graph
        .apply(GraphTransaction::with_mutations(
            0,
            [GraphMutation::Add {
                node_id,
                node,
                position: 0,
            }],
        ))
        .unwrap();

    for work in [
        EngineWorkKind::Playback,
        EngineWorkKind::Rendering,
        EngineWorkKind::Export,
    ] {
        let resolution = supervisor.resolve_work(work, &snapshot);
        assert_eq!(resolution.missing_node_count(), 1);
        let error = resolution.require_evaluable().unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Unavailable);
        assert_eq!(error.recoverability(), Recoverability::Degraded);
    }

    let error = supervisor
        .enable(&identity, &CapabilitySet::new(std::iter::empty()))
        .unwrap_err();
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(
        supervisor.plugin_status(&identity).unwrap().lifecycle(),
        OfxPluginLifecycle::Disabled
    );

    let authorized = CapabilitySet::new([media, network]);
    let error = supervisor.enable(&identity, &authorized).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(error.recoverability(), Recoverability::Retryable);
    assert_eq!(
        supervisor.plugin_status(&identity).unwrap().lifecycle(),
        OfxPluginLifecycle::Faulted
    );
    assert!(supervisor.failures().iter().any(|failure| {
        failure.recoverability() == Recoverability::Retryable
            && failure.recommended_action() == PluginFailureAction::Retry
            && failure.message().contains("deadline")
    }));

    supervisor.recover(&identity).unwrap();
    supervisor.enable(&identity, &authorized).unwrap();
    assert_eq!(
        supervisor.plugin_status(&identity).unwrap().lifecycle(),
        OfxPluginLifecycle::Ready
    );
    assert_eq!(supervisor.active_registry().len(), 1);

    let mut sampler = |_time: OfxTime,
                       _address: ParameterAddress,
                       literal: &TypedParameterValue<GraphValue<()>>|
     -> Result<GraphValue<()>> { Ok(literal.payload().clone()) };
    supervisor
        .create_instance(
            &identity,
            OfxContext::Filter,
            &snapshot,
            node_id,
            OfxTime::new(0.0).unwrap(),
            &mut sampler,
        )
        .unwrap();
    let error = supervisor
        .render(
            &identity,
            OfxInstanceKey::new(snapshot.graph_id(), node_id),
            &snapshot,
            OfxTime::new(0.0).unwrap(),
            &mut sampler,
            OfxRenderWindow::new(0, 0, 1920, 1080).unwrap(),
            [
                OfxImageResource::new("Source", "input-token").unwrap(),
                OfxImageResource::new("Output", "output-token").unwrap(),
            ],
        )
        .unwrap_err();
    assert_eq!(error.recoverability(), Recoverability::Retryable);
    let status = supervisor.plugin_status(&identity).unwrap();
    assert_eq!(status.lifecycle(), OfxPluginLifecycle::Quarantined);
    assert_eq!(status.consecutive_failures(), 2);
    assert!(supervisor.active_registry().is_empty());

    let degraded_revisions = [
        EngineWorkKind::Playback,
        EngineWorkKind::Rendering,
        EngineWorkKind::Export,
    ]
    .map(|work| {
        let resolution = supervisor.resolve_work(work, &snapshot);
        assert_eq!(resolution.missing_node_count(), 1);
        assert_eq!(
            resolution.require_evaluable().unwrap_err().recoverability(),
            Recoverability::Degraded
        );
        resolution.supervisor_revision()
    });
    assert_eq!(degraded_revisions, [degraded_revisions[0]; 3]);

    supervisor.clear_quarantine(&identity).unwrap();
    supervisor.enable(&identity, &authorized).unwrap();
    let healthy_revisions = [
        EngineWorkKind::Playback,
        EngineWorkKind::Rendering,
        EngineWorkKind::Export,
    ]
    .map(|work| {
        let resolution = supervisor.resolve_work(work, &snapshot);
        assert_eq!(resolution.missing_node_count(), 0);
        assert_eq!(resolution.require_evaluable().unwrap(), &snapshot);
        resolution.supervisor_revision()
    });
    assert_eq!(healthy_revisions, [healthy_revisions[0]; 3]);
}
