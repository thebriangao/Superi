use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::settings::{CapabilityId, CapabilitySet, SemanticVersion};
use superi_effects::authoring::{
    EffectInstanceBindings, EffectNodeDefinition, EffectParameterBinding,
};
use superi_effects::ofx::{
    IsolatedOfxAdapter, OfxAction, OfxAdapterContract, OfxAdapterIsolation, OfxClipDescriptor,
    OfxClipDirection, OfxContext, OfxContextDescriptor, OfxCreateInstanceRequest,
    OfxHostCapabilities, OfxImageAccess, OfxImageResource, OfxInstanceKey, OfxLoadRequest,
    OfxParameterDescriptor, OfxParameterKind, OfxParameterValue, OfxPluginCapabilities,
    OfxPluginDescriptor, OfxPluginHost, OfxPluginIdentity, OfxPluginLifecycle, OfxRenderReceipt,
    OfxRenderRequest, OfxRenderThreadSafety, OfxRenderWindow, OfxTime,
};
use superi_graph::expr::{
    ExpressionVariableName, ParameterAddress, ParameterDriver, ParameterExpression,
    ParameterReference,
};
use superi_graph::ids::{GraphId, NodeId, ParameterId, PortId};
use superi_graph::missing::resolve_graph;
use superi_graph::mutate::{
    EditableGraph, GraphMutation, GraphTransaction, InstancePort, TypedParameterValue,
};
use superi_graph::node::{ParameterName, PortName};
use superi_graph::serialize::{deserialize_graph, serialize_graph};
use superi_graph::value::GraphValue;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RenderMode {
    Succeed,
    Fail(u32),
    Panic,
}

#[derive(Clone)]
struct MockControls {
    actions: Arc<Mutex<Vec<OfxAction>>>,
    load_requests: Arc<Mutex<Vec<OfxLoadRequest>>>,
    last_create: Arc<Mutex<Option<OfxCreateInstanceRequest>>>,
    last_render: Arc<Mutex<Option<OfxRenderRequest>>>,
    render_mode: Arc<Mutex<RenderMode>>,
}

impl MockControls {
    fn new() -> Self {
        Self {
            actions: Arc::new(Mutex::new(Vec::new())),
            load_requests: Arc::new(Mutex::new(Vec::new())),
            last_create: Arc::new(Mutex::new(None)),
            last_render: Arc::new(Mutex::new(None)),
            render_mode: Arc::new(Mutex::new(RenderMode::Succeed)),
        }
    }

    fn actions(&self) -> Vec<OfxAction> {
        self.actions.lock().unwrap().clone()
    }

    fn set_render_mode(&self, mode: RenderMode) {
        *self.render_mode.lock().unwrap() = mode;
    }
}

struct MockAdapter {
    contract: OfxAdapterContract,
    plugin: OfxPluginDescriptor,
    contexts: BTreeMap<OfxContext, OfxContextDescriptor>,
    controls: MockControls,
}

impl IsolatedOfxAdapter for MockAdapter {
    fn contract(&self) -> OfxAdapterContract {
        self.contract.clone()
    }

    fn load(&mut self, request: OfxLoadRequest) -> Result<()> {
        self.controls.actions.lock().unwrap().push(OfxAction::Load);
        self.controls.load_requests.lock().unwrap().push(request);
        Ok(())
    }

    fn describe(&mut self) -> Result<OfxPluginDescriptor> {
        self.controls
            .actions
            .lock()
            .unwrap()
            .push(OfxAction::Describe);
        Ok(self.plugin.clone())
    }

    fn describe_in_context(&mut self, context: OfxContext) -> Result<OfxContextDescriptor> {
        self.controls
            .actions
            .lock()
            .unwrap()
            .push(OfxAction::DescribeInContext);
        self.contexts.get(&context).cloned().ok_or_else(|| {
            Error::new(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "mock context is absent",
            )
        })
    }

    fn create_instance(&mut self, request: OfxCreateInstanceRequest) -> Result<()> {
        self.controls
            .actions
            .lock()
            .unwrap()
            .push(OfxAction::CreateInstance);
        *self.controls.last_create.lock().unwrap() = Some(request);
        Ok(())
    }

    fn render(&mut self, request: OfxRenderRequest) -> Result<OfxRenderReceipt> {
        self.controls
            .actions
            .lock()
            .unwrap()
            .push(OfxAction::Render);
        *self.controls.last_render.lock().unwrap() = Some(request.clone());
        let mut mode = self.controls.render_mode.lock().unwrap();
        match *mode {
            RenderMode::Succeed => Ok(OfxRenderReceipt::new(
                request
                    .image("Output")
                    .expect("validated output binding")
                    .resource_token(),
            )?),
            RenderMode::Fail(remaining) => {
                *mode = RenderMode::Fail(remaining.saturating_sub(1));
                Err(Error::new(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "mock worker render failed",
                ))
            }
            RenderMode::Panic => panic!("mock worker adapter panic"),
        }
    }

    fn destroy_instance(&mut self, _key: OfxInstanceKey) -> Result<()> {
        self.controls
            .actions
            .lock()
            .unwrap()
            .push(OfxAction::DestroyInstance);
        Ok(())
    }

    fn unload(&mut self) -> Result<()> {
        self.controls
            .actions
            .lock()
            .unwrap()
            .push(OfxAction::Unload);
        Ok(())
    }

    fn restart_worker(&mut self) -> Result<()> {
        self.controls
            .actions
            .lock()
            .unwrap()
            .push(OfxAction::RestartWorker);
        Ok(())
    }
}

fn empty_capabilities() -> CapabilitySet {
    CapabilitySet::new(std::iter::empty())
}

fn worker_contract() -> OfxAdapterContract {
    OfxAdapterContract::new(
        OfxAdapterIsolation::WorkerProcess,
        1,
        4 * 1024 * 1024,
        30_000,
        true,
    )
}

fn double_parameter(name: &str, default: f64) -> OfxParameterDescriptor {
    OfxParameterDescriptor::new(
        name,
        name,
        format!("Editable {name} parameter."),
        OfxParameterKind::Double,
        OfxParameterValue::double(default).unwrap(),
        true,
        true,
        Vec::<String>::new(),
    )
    .unwrap()
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
        [double_parameter("Gain", 1.0), double_parameter("Base", 2.0)],
    )
    .unwrap()
}

fn fixture(permissions: CapabilitySet) -> (MockAdapter, MockControls, OfxPluginIdentity) {
    let controls = MockControls::new();
    let identity =
        OfxPluginIdentity::new("com.example.superi-gain", SemanticVersion::new(1, 2, 0)).unwrap();
    let plugin = OfxPluginDescriptor::new(
        identity.clone(),
        "Superi Gain",
        "A deterministic fixture image effect.",
        [OfxContext::Filter],
        OfxPluginCapabilities::new(
            true,
            true,
            false,
            OfxRenderThreadSafety::InstanceSafe,
            false,
        ),
        permissions,
    )
    .unwrap();
    let context = filter_context();
    let adapter = MockAdapter {
        contract: worker_contract(),
        plugin,
        contexts: BTreeMap::from([(OfxContext::Filter, context)]),
        controls: controls.clone(),
    };
    (adapter, controls, identity)
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
        [
            EffectParameterBinding::new(
                ParameterId::from_raw(20),
                ParameterName::new("gain").unwrap(),
            ),
            EffectParameterBinding::new(
                ParameterId::from_raw(21),
                ParameterName::new("base").unwrap(),
            ),
        ],
    )
}

fn definition_and_graph(
    host: &OfxPluginHost<MockAdapter>,
) -> (
    EffectNodeDefinition<GraphValue<()>>,
    EditableGraph<GraphValue<()>>,
    NodeId,
) {
    let definition = host.definition::<()>(OfxContext::Filter).unwrap();
    let node = definition
        .instantiate(bindings(), std::iter::empty())
        .unwrap();
    let node_id = NodeId::from_raw(100);
    let mut graph = EditableGraph::new(GraphId::from_raw(50));
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [GraphMutation::Add {
                node_id,
                node,
                position: 0,
            }],
        ))
        .unwrap();
    (definition, graph, node_id)
}

#[test]
fn scan_follows_ofx_lifecycle_and_projects_inspectable_graph_definitions() {
    let (adapter, controls, identity) = fixture(empty_capabilities());
    let host = OfxPluginHost::scan(adapter, 3).unwrap();

    assert_eq!(host.lifecycle(), OfxPluginLifecycle::Disabled);
    assert_eq!(host.plugin().identity(), &identity);
    assert_eq!(host.contexts().count(), 1);
    assert_eq!(host.adapter_contract(), &worker_contract());
    assert_eq!(host.capabilities(), &OfxHostCapabilities::openfx_1_5_1());
    assert_eq!(
        controls.actions(),
        [
            OfxAction::Load,
            OfxAction::Describe,
            OfxAction::DescribeInContext,
            OfxAction::Unload,
        ]
    );

    let load_requests = controls.load_requests.lock().unwrap();
    assert_eq!(load_requests.len(), 1);
    assert!(load_requests[0].is_scan());
    assert!(load_requests[0].granted_permissions().is_empty());
    drop(load_requests);

    let definition = host.definition::<()>(OfxContext::Filter).unwrap();
    assert_eq!(
        definition.schema().id().node_type().as_str(),
        "ofx.com.example.superi-gain.filter"
    );
    assert_eq!(
        definition.schema().id().schema_version(),
        identity.version()
    );
    assert_eq!(definition.metadata().label(), "Superi Gain (Filter)");
    assert_eq!(definition.metadata().category(), "OpenFX");
    assert!(definition
        .schema()
        .parameter(&ParameterName::new("gain").unwrap())
        .unwrap()
        .is_animatable());

    assert_eq!(host.discovered_catalog::<()>().unwrap().snapshot().len(), 1);
    assert_eq!(host.active_catalog::<()>().unwrap().snapshot().len(), 0);
}

#[test]
fn graph_state_round_trips_and_plugin_availability_never_rewrites_authored_state() {
    let (adapter, _controls, _identity) = fixture(empty_capabilities());
    let mut host = OfxPluginHost::scan(adapter, 3).unwrap();
    let (_definition, graph, node_id) = definition_and_graph(&host);
    let snapshot = graph.snapshot();
    let document = serialize_graph(&snapshot).unwrap();

    let disabled_registry = host.active_catalog::<()>().unwrap().snapshot();
    let disabled = resolve_graph(&snapshot, disabled_registry.node_schemas());
    assert_eq!(disabled.missing_node_count(), 1);
    assert_eq!(
        disabled.node(node_id).unwrap().node(),
        snapshot.node(node_id).unwrap()
    );

    host.enable(&empty_capabilities()).unwrap();
    let active_registry = host.active_catalog::<()>().unwrap().snapshot();
    let active = resolve_graph(&snapshot, active_registry.node_schemas());
    assert!(active.is_evaluable());
    assert_eq!(serialize_graph(active.graph()).unwrap(), document);

    host.disable().unwrap();
    let disabled_again = host.active_catalog::<()>().unwrap().snapshot();
    assert_eq!(
        resolve_graph(&snapshot, disabled_again.node_schemas()).missing_node_count(),
        1
    );
    let loaded = deserialize_graph::<GraphValue<()>>(&document).unwrap();
    assert_eq!(loaded.graph().snapshot(), snapshot);
    assert_eq!(
        serialize_graph(&loaded.graph().snapshot()).unwrap(),
        document
    );
}

#[test]
fn render_samples_timeline_values_before_graph_expressions_and_binds_exact_clips() {
    let (adapter, controls, _identity) = fixture(empty_capabilities());
    let mut host = OfxPluginHost::scan(adapter, 3).unwrap();
    host.enable(&empty_capabilities()).unwrap();
    let (definition, mut graph, node_id) = definition_and_graph(&host);
    let scalar_type = definition
        .schema()
        .parameter(&ParameterName::new("gain").unwrap())
        .unwrap()
        .value_type()
        .clone();
    let base = ParameterAddress::new(node_id, ParameterId::from_raw(21));
    let gain = ParameterAddress::new(node_id, ParameterId::from_raw(20));
    let expression = ParameterExpression::compile(
        "base * 2",
        [(
            ExpressionVariableName::new("base").unwrap(),
            ParameterReference::new(base, scalar_type.clone()),
        )],
    )
    .unwrap();
    graph
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameterDriver {
                target: gain,
                driver: ParameterDriver::expression(scalar_type, expression),
            }],
        ))
        .unwrap();

    let mut sampler = |time: OfxTime,
                       _address: ParameterAddress,
                       literal: &TypedParameterValue<GraphValue<()>>|
     -> Result<GraphValue<()>> {
        match literal.payload().as_scalar() {
            Some(value) => GraphValue::scalar(value * time.get()),
            None => Ok(literal.payload().clone()),
        }
    };
    let snapshot = graph.snapshot();
    host.create_instance(
        OfxContext::Filter,
        &snapshot,
        node_id,
        OfxTime::new(1.0).unwrap(),
        &mut sampler,
    )
    .unwrap();
    let key = OfxInstanceKey::new(snapshot.graph_id(), node_id);
    let error = host
        .render(
            key,
            &snapshot,
            OfxTime::new(3.0).unwrap(),
            &mut sampler,
            OfxRenderWindow::new(0, 0, 1920, 1080).unwrap(),
            [OfxImageResource::new("Source", "input-token").unwrap()],
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(host.lifecycle(), OfxPluginLifecycle::Ready);
    assert_eq!(host.status().total_failures(), 0);

    let receipt = host
        .render(
            key,
            &snapshot,
            OfxTime::new(3.0).unwrap(),
            &mut sampler,
            OfxRenderWindow::new(0, 0, 1920, 1080).unwrap(),
            [
                OfxImageResource::new("Source", "input-token").unwrap(),
                OfxImageResource::new("Output", "output-token").unwrap(),
            ],
        )
        .unwrap();
    assert_eq!(receipt.output_resource(), "output-token");

    let request = controls.last_render.lock().unwrap().clone().unwrap();
    assert_eq!(request.graph_revision(), 2);
    assert_eq!(request.parameter("Base").unwrap().as_double(), Some(6.0));
    assert_eq!(request.parameter("Gain").unwrap().as_double(), Some(12.0));
    assert_eq!(
        request.image("Source").unwrap().access(),
        OfxImageAccess::ReadOnly
    );
    assert_eq!(
        request.image("Output").unwrap().access(),
        OfxImageAccess::WriteOnly
    );

    host.destroy_instance(key).unwrap();
    host.disable().unwrap();
    assert_eq!(
        controls.actions(),
        [
            OfxAction::Load,
            OfxAction::Describe,
            OfxAction::DescribeInContext,
            OfxAction::Unload,
            OfxAction::Load,
            OfxAction::CreateInstance,
            OfxAction::Render,
            OfxAction::DestroyInstance,
            OfxAction::Unload,
        ]
    );
}

#[test]
fn permissions_fail_closed_and_repeated_worker_failures_require_explicit_control() {
    let network = CapabilityId::new("superi.permission.network").unwrap();
    let requested = CapabilitySet::new([network.clone()]);
    let (adapter, controls, _identity) = fixture(requested.clone());
    let mut host = OfxPluginHost::scan(adapter, 2).unwrap();

    let error = host.enable(&empty_capabilities()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::PermissionDenied);
    assert_eq!(host.lifecycle(), OfxPluginLifecycle::Disabled);
    assert_eq!(controls.actions().len(), 4);

    host.enable(&requested).unwrap();
    assert!(host.status().granted_permissions().contains(&network));
    let (_definition, graph, node_id) = definition_and_graph(&host);
    let snapshot = graph.snapshot();
    let mut sampler = |_time: OfxTime,
                       _address: ParameterAddress,
                       literal: &TypedParameterValue<GraphValue<()>>|
     -> Result<GraphValue<()>> { Ok(literal.payload().clone()) };
    host.create_instance(
        OfxContext::Filter,
        &snapshot,
        node_id,
        OfxTime::new(1.0).unwrap(),
        &mut sampler,
    )
    .unwrap();
    controls.set_render_mode(RenderMode::Fail(2));
    let key = OfxInstanceKey::new(snapshot.graph_id(), node_id);
    let resources = || {
        [
            OfxImageResource::new("Source", "input-token").unwrap(),
            OfxImageResource::new("Output", "output-token").unwrap(),
        ]
    };
    let error = host
        .render(
            key,
            &snapshot,
            OfxTime::new(1.0).unwrap(),
            &mut sampler,
            OfxRenderWindow::new(0, 0, 16, 16).unwrap(),
            resources(),
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unavailable);
    assert_eq!(host.lifecycle(), OfxPluginLifecycle::Faulted);
    assert_eq!(host.status().consecutive_failures(), 1);
    assert_eq!(
        host.status().last_failure().unwrap().action(),
        OfxAction::Render
    );

    host.recover().unwrap();
    assert_eq!(host.lifecycle(), OfxPluginLifecycle::Disabled);
    assert_eq!(host.status().consecutive_failures(), 1);
    host.enable(&requested).unwrap();
    host.create_instance(
        OfxContext::Filter,
        &snapshot,
        node_id,
        OfxTime::new(1.0).unwrap(),
        &mut sampler,
    )
    .unwrap();
    let error = host
        .render(
            key,
            &snapshot,
            OfxTime::new(1.0).unwrap(),
            &mut sampler,
            OfxRenderWindow::new(0, 0, 16, 16).unwrap(),
            resources(),
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unavailable);
    assert_eq!(host.lifecycle(), OfxPluginLifecycle::Quarantined);
    assert_eq!(host.active_catalog::<()>().unwrap().snapshot().len(), 0);

    host.clear_quarantine().unwrap();
    assert_eq!(host.lifecycle(), OfxPluginLifecycle::Disabled);
    assert_eq!(host.status().consecutive_failures(), 0);
    assert_eq!(host.status().total_failures(), 2);
}

#[test]
fn adapter_panics_are_contained_and_fault_the_plugin() {
    let (adapter, controls, _identity) = fixture(empty_capabilities());
    let mut host = OfxPluginHost::scan(adapter, 3).unwrap();
    host.enable(&empty_capabilities()).unwrap();
    let (_definition, graph, node_id) = definition_and_graph(&host);
    let snapshot = graph.snapshot();
    let mut sampler = |_time: OfxTime,
                       _address: ParameterAddress,
                       literal: &TypedParameterValue<GraphValue<()>>|
     -> Result<GraphValue<()>> { Ok(literal.payload().clone()) };
    host.create_instance(
        OfxContext::Filter,
        &snapshot,
        node_id,
        OfxTime::new(1.0).unwrap(),
        &mut sampler,
    )
    .unwrap();
    controls.set_render_mode(RenderMode::Panic);
    let error = host
        .render(
            OfxInstanceKey::new(snapshot.graph_id(), node_id),
            &snapshot,
            OfxTime::new(1.0).unwrap(),
            &mut sampler,
            OfxRenderWindow::new(0, 0, 16, 16).unwrap(),
            [
                OfxImageResource::new("Source", "input-token").unwrap(),
                OfxImageResource::new("Output", "output-token").unwrap(),
            ],
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Internal);
    assert_eq!(host.lifecycle(), OfxPluginLifecycle::Faulted);
    assert_eq!(
        host.status().last_failure().unwrap().action(),
        OfxAction::Render
    );
    host.recover().unwrap();
    assert_eq!(host.lifecycle(), OfxPluginLifecycle::Disabled);
}

#[test]
fn standard_transition_state_is_host_managed_and_graph_animatable() {
    let controls = MockControls::new();
    let identity = OfxPluginIdentity::new(
        "com.example.superi-transition",
        SemanticVersion::new(1, 0, 0),
    )
    .unwrap();
    let plugin = OfxPluginDescriptor::new(
        identity,
        "Superi Transition",
        "A standard transition fixture.",
        [OfxContext::Transition],
        OfxPluginCapabilities::new(
            true,
            true,
            false,
            OfxRenderThreadSafety::InstanceSafe,
            false,
        ),
        empty_capabilities(),
    )
    .unwrap();
    let transition = OfxParameterDescriptor::new(
        "Transition",
        "Transition",
        "Host-managed transition progress.",
        OfxParameterKind::Double,
        OfxParameterValue::double(0.0).unwrap(),
        false,
        true,
        Vec::<String>::new(),
    )
    .unwrap();
    let clips = [
        ("SourceFrom", OfxClipDirection::Input),
        ("SourceTo", OfxClipDirection::Input),
        ("Output", OfxClipDirection::Output),
    ]
    .map(|(name, direction)| {
        OfxClipDescriptor::new(name, name, format!("{name} image."), direction, false).unwrap()
    });
    let context = OfxContextDescriptor::new(OfxContext::Transition, clips, [transition]).unwrap();
    assert!(context.is_host_managed_parameter("Transition"));
    let adapter = MockAdapter {
        contract: worker_contract(),
        plugin,
        contexts: BTreeMap::from([(OfxContext::Transition, context)]),
        controls,
    };
    let host = OfxPluginHost::scan(adapter, 3).unwrap();
    let definition = host.definition::<()>(OfxContext::Transition).unwrap();
    assert!(definition
        .schema()
        .parameter(&ParameterName::new("transition").unwrap())
        .unwrap()
        .is_animatable());
}

#[test]
fn unsafe_adapters_and_ambiguous_or_incomplete_contexts_fail_during_validation() {
    let invalid_contract =
        OfxAdapterContract::new(OfxAdapterIsolation::InProcess, 1, 1024, 1000, true);
    let (mut adapter, _controls, _identity) = fixture(empty_capabilities());
    adapter.contract = invalid_contract;
    let error = OfxPluginHost::scan(adapter, 3).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);

    let error = OfxContextDescriptor::new(
        OfxContext::Filter,
        [
            OfxClipDescriptor::new(
                "Source",
                "Source",
                "Required input.",
                OfxClipDirection::Input,
                false,
            )
            .unwrap(),
            OfxClipDescriptor::new(
                "source",
                "Alternate",
                "Ambiguous input.",
                OfxClipDirection::Input,
                true,
            )
            .unwrap(),
            OfxClipDescriptor::new(
                "Output",
                "Output",
                "Required output.",
                OfxClipDirection::Output,
                false,
            )
            .unwrap(),
        ],
        [double_parameter("Gain", 1.0)],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let error = OfxContextDescriptor::new(
        OfxContext::Filter,
        [OfxClipDescriptor::new(
            "Output",
            "Output",
            "Required output.",
            OfxClipDirection::Output,
            false,
        )
        .unwrap()],
        [double_parameter("Gain", 1.0)],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
}
