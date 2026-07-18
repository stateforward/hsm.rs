use stateforward_hsm::*;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::env;
use std::fmt;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

type ConfResult<T> = std::result::Result<T, ConfError>;
type AttributeEventMap = BTreeMap<String, String>;
type AttributeTypeMap = HashMap<String, String>;

const SUPPORTED_FEATURES: &[&str] = &[
    "core",
    "entry",
    "exit",
    "effect",
    "initial",
    "final",
    "source",
    "attribute",
    "snapshot",
    "nested",
    "paths",
    "path_resolution",
    "entry_point",
    "exit_point",
    "on_set",
    "validation",
    "guard",
    "choice",
    "root_transition",
    "submachine",
    "redefine",
    "history",
    "shallow_history",
    "deep_history",
    "history_default",
    "selection_order",
    "completion",
    "transition_kind",
    "external",
    "internal",
    "local",
    "self",
    "event",
    "event_data",
    "event_ownership",
    "error",
    "activity",
    "async",
    "defer",
    "timer",
    "after",
    "every",
    "at",
    "timer_behavior",
    "cancellation",
    "behavior_attr",
    "operation",
    "on_call",
    "when",
    "queue",
    "queue_order",
    "reentrancy",
    "dispatch_to",
    "model_registry",
    "runtime_context",
    "lifecycle",
    "restart",
    "stop",
    "broadcast",
    "multi_target",
    "group",
];

#[derive(Debug)]
enum ConfError {
    Fail(String),
    Skip(String),
}

impl fmt::Display for ConfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfError::Fail(message) | ConfError::Skip(message) => formatter.write_str(message),
        }
    }
}

impl From<std::io::Error> for ConfError {
    fn from(error: std::io::Error) -> Self {
        ConfError::Fail(error.to_string())
    }
}

impl From<HsmError> for ConfError {
    fn from(error: HsmError) -> Self {
        match error {
            HsmError::Runtime(message) => ConfError::Fail(message),
            other => ConfError::Fail(other.to_string()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    fn object(entries: Vec<(&str, Json)>) -> Self {
        Json::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
        )
    }
}

#[derive(Clone, Debug)]
struct RuntimeIssue {
    skip: bool,
    code: Option<String>,
    message: String,
}

#[derive(Clone, Debug)]
enum TimerSource {
    DurationMs(u64),
    TimeMs(u64),
    Attribute(String),
    Behavior(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConformanceTimerKind {
    After,
    At,
    Every,
}

impl ConformanceTimerKind {
    fn constraint_name(self) -> &'static str {
        match self {
            ConformanceTimerKind::After => "after",
            ConformanceTimerKind::At => "at",
            ConformanceTimerKind::Every => "every",
        }
    }

    fn is_timepoint(self) -> bool {
        self == ConformanceTimerKind::At
    }
}

#[derive(Clone, Debug)]
struct TimerSpec {
    source: TimerSource,
}

struct ClockSleeper {
    instance_id: String,
    context_key: Option<usize>,
    context: Option<Context>,
    trace: ConformanceInstance,
    trace_timer_fired: bool,
    due: u64,
    order: u64,
    wake: tokio::sync::oneshot::Sender<()>,
}

#[derive(Default)]
struct LogicalClock {
    now_ms: Mutex<u64>,
    order: Mutex<u64>,
    sleepers: Mutex<Vec<ClockSleeper>>,
}

impl LogicalClock {
    fn now(&self) -> u64 {
        *self.now_ms.lock().unwrap()
    }

    fn sleep(
        self: &Arc<Self>,
        duration: Duration,
        clock_name: Option<&str>,
        trace: ConformanceInstance,
        trace_timer_scheduled: bool,
        trace_timer_fired: bool,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let millis = duration.as_millis().min(u128::from(u64::MAX)) as u64;
        if trace_timer_scheduled {
            trace.push_trace(Json::object(vec![(
                "type",
                Json::String("timer_scheduled".to_string()),
            )]));
        }
        match clock_name {
            Some("trace_no_sleep") if millis > 0 => {
                trace.push_trace(Json::object(vec![
                    ("type", Json::String("trace".to_string())),
                    ("value", Json::String(format!("clock:sleep:{millis}"))),
                ]));
            }
            Some("trace_yield_sleep") if millis > 0 => {
                trace.push_trace(Json::object(vec![
                    ("type", Json::String("trace".to_string())),
                    ("value", Json::String(format!("clock:sleep:{millis}"))),
                ]));
            }
            Some("trace_nonzero_sleep") if millis > 0 => {
                trace.push_trace(Json::object(vec![
                    ("type", Json::String("trace".to_string())),
                    ("value", Json::String("clock:sleep:nonzero".to_string())),
                ]));
            }
            _ => {}
        }

        let registration = stateforward_hsm::behavior_context::current_timer_registration();
        let (instance_id, context_key, context) = match registration {
            Some(registration) => (
                registration.instance_id,
                Some(registration.context_key),
                Some(registration.context),
            ),
            None => ("default".to_string(), None, None),
        };
        let (wake, wait) = tokio::sync::oneshot::channel();
        let now = self.now();
        let due = if matches!(clock_name, Some("trace_no_sleep" | "trace_nonzero_sleep")) {
            now
        } else {
            now.saturating_add(millis)
        };
        let order = {
            let mut order = self.order.lock().unwrap();
            let current = *order;
            *order = order.saturating_add(1);
            current
        };
        self.sleepers.lock().unwrap().push(ClockSleeper {
            instance_id,
            context_key,
            context,
            trace,
            trace_timer_fired,
            due,
            order,
            wake,
        });
        Box::pin(async move {
            let _ = wait.await;
        })
    }

    async fn advance(&self, millis: u64) {
        {
            let mut now = self.now_ms.lock().unwrap();
            *now = now.saturating_add(millis);
        }
        loop {
            let sleeper = self.pop_due_sleeper();
            let Some(sleeper) = sleeper else {
                break;
            };
            if sleeper.trace_timer_fired {
                sleeper.trace.push_trace(Json::object(vec![(
                    "type",
                    Json::String("timer_fired".to_string()),
                )]));
            }
            let has_due_peer = sleeper
                .context_key
                .is_some_and(|context_key| self.has_due_sleeper_for_context(context_key));
            let context = sleeper.context.clone();
            let _ = sleeper.wake.send(());
            flush_timer_wake(context.as_ref(), has_due_peer).await;
        }
    }

    fn pop_due_sleeper(&self) -> Option<ClockSleeper> {
        let now = self.now();
        let mut sleepers = self.sleepers.lock().unwrap();
        sleepers.retain(|sleeper| {
            !sleeper
                .context
                .as_ref()
                .is_some_and(|context| context.is_cancelled())
        });
        let index = sleepers
            .iter()
            .enumerate()
            .filter(|(_, sleeper)| sleeper.due <= now)
            .min_by_key(|(_, sleeper)| (sleeper.due, sleeper.order))
            .map(|(index, _)| index)?;
        Some(sleepers.remove(index))
    }

    fn cancel_for_instance(&self, instance_id: &str) -> usize {
        let mut sleepers = self.sleepers.lock().unwrap();
        let before = sleepers.len();
        sleepers.retain(|sleeper| {
            if sleeper.instance_id == instance_id {
                if let Some(context) = &sleeper.context {
                    context.cancel();
                }
                false
            } else {
                true
            }
        });
        before - sleepers.len()
    }

    fn remove_cancelled_for_instance(&self, instance_id: &str) -> usize {
        let mut sleepers = self.sleepers.lock().unwrap();
        let before = sleepers.len();
        sleepers.retain(|sleeper| {
            !(sleeper.instance_id == instance_id
                && sleeper
                    .context
                    .as_ref()
                    .is_some_and(|context| context.is_cancelled()))
        });
        before - sleepers.len()
    }

    fn has_due_sleeper_for_context(&self, context_key: usize) -> bool {
        let now = self.now();
        self.sleepers.lock().unwrap().iter().any(|sleeper| {
            sleeper.due <= now
                && sleeper.context_key == Some(context_key)
                && !sleeper
                    .context
                    .as_ref()
                    .is_some_and(|context| context.is_cancelled())
        })
    }
}

async fn flush_timer_wake(context: Option<&Context>, has_due_peer: bool) {
    if has_due_peer {
        if let Some(context) = context {
            for _ in 0..512 {
                if context.is_cancelled() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        }
    }
    flush_async_work().await;
}

async fn flush_async_work() {
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    tokio::time::sleep(Duration::ZERO).await;
}

static TIMER_SPECS: OnceLock<Mutex<HashMap<String, TimerSpec>>> = OnceLock::new();
static CURRENT_CLOCK: OnceLock<Mutex<Option<Arc<LogicalClock>>>> = OnceLock::new();
static ACTIVITY_TRACE_TYPES: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();

fn timer_specs() -> &'static Mutex<HashMap<String, TimerSpec>> {
    TIMER_SPECS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn current_clock() -> &'static Mutex<Option<Arc<LogicalClock>>> {
    CURRENT_CLOCK.get_or_init(|| Mutex::new(None))
}

fn activity_trace_types() -> &'static Mutex<BTreeSet<String>> {
    ACTIVITY_TRACE_TYPES.get_or_init(|| Mutex::new(BTreeSet::new()))
}

fn set_current_clock(clock: Arc<LogicalClock>) {
    *current_clock().lock().unwrap() = Some(clock);
}

fn set_activity_trace_options(expect: &BTreeMap<String, Json>) {
    let mut types = activity_trace_types().lock().unwrap();
    types.clear();
    for trace_type in ["activity_done", "activity_cancel"] {
        if expected_trace_contains(expect, trace_type) {
            types.insert(trace_type.to_string());
        }
    }
}

fn activity_trace_enabled(trace_type: &str) -> bool {
    activity_trace_types().lock().unwrap().contains(trace_type)
}

fn reset_case_state() {
    timer_specs().lock().unwrap().clear();
    *current_clock().lock().unwrap() = None;
    activity_trace_types().lock().unwrap().clear();
}

fn current_clock_now() -> u64 {
    current_clock()
        .lock()
        .unwrap()
        .as_ref()
        .map(|clock| clock.now())
        .unwrap_or(0)
}

fn current_clock_handle() -> Option<Arc<LogicalClock>> {
    current_clock().lock().unwrap().clone()
}

fn register_timer_spec(event_name: String, spec: TimerSpec) {
    timer_specs().lock().unwrap().insert(event_name, spec);
}

fn timer_spec_for_event(event_name: &str) -> Option<TimerSpec> {
    let specs = timer_specs().lock().unwrap();
    if let Some(spec) = specs.get(event_name) {
        return Some(spec.clone());
    }
    let event_suffix = timer_event_suffix(event_name);
    specs
        .iter()
        .find(|(name, _)| path_has_suffix(&event_suffix, &timer_event_suffix(name)))
        .map(|(_, spec)| spec.clone())
}

fn timer_event_suffix(path: &str) -> String {
    path_without_root(path).unwrap_or_else(|| path.trim_start_matches('/').to_string())
}

fn path_has_suffix(path: &str, suffix: &str) -> bool {
    path == suffix || path.ends_with(&format!("/{suffix}"))
}

fn path_without_root(path: &str) -> Option<String> {
    let mut parts = path.trim_start_matches('/').splitn(2, '/');
    parts.next()?;
    parts.next().map(str::to_string)
}

fn is_registered_timer_event(event_name: &str) -> bool {
    timer_spec_for_event(event_name).is_some()
}

fn trace_entry_is_type(entry: &Json, entry_type: &str) -> bool {
    matches!(
        entry,
        Json::Object(object)
            if matches!(object.get("type"), Some(Json::String(value)) if value == entry_type)
    )
}

fn conformance_timer_duration(
    ctx: &Context,
    inst: &ConformanceInstance,
    event: &Event,
) -> Duration {
    let Some(spec) = timer_spec_for_event(&event.name) else {
        inst.error(
            "timer_error".to_string(),
            format!("missing timer spec for {}", event.name),
        );
        stateforward_hsm::behavior_context::request_abort();
        return Duration::ZERO;
    };
    match timer_source_millis(ctx, inst, event, &spec) {
        Some(millis) => duration_from_millis(millis),
        None => Duration::ZERO,
    }
}

fn conformance_timer_timepoint(
    ctx: &Context,
    inst: &ConformanceInstance,
    event: &Event,
) -> SystemTime {
    let Some(spec) = timer_spec_for_event(&event.name) else {
        inst.error(
            "timer_error".to_string(),
            format!("missing timer spec for {}", event.name),
        );
        stateforward_hsm::behavior_context::request_abort();
        return SystemTime::now();
    };
    let millis = timer_source_millis(ctx, inst, event, &spec).unwrap_or(0);
    let remaining = millis.saturating_sub(current_clock_now());
    SystemTime::now() + duration_from_millis(remaining)
}

fn duration_from_millis(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

fn timer_source_millis(
    ctx: &Context,
    inst: &ConformanceInstance,
    event: &Event,
    spec: &TimerSpec,
) -> Option<u64> {
    match &spec.source {
        TimerSource::DurationMs(millis) | TimerSource::TimeMs(millis) => Some(*millis),
        TimerSource::Attribute(name) => timer_attribute_millis(ctx, inst, event, name),
        TimerSource::Behavior(name) => timer_behavior_millis(ctx, inst, event, name),
    }
}

fn timer_behavior_millis(
    ctx: &Context,
    inst: &ConformanceInstance,
    event: &Event,
    behavior_id: &str,
) -> Option<u64> {
    let Some(program) = inst.behaviors.get(behavior_id).cloned() else {
        inst.fail(format!("missing timer behavior \"{behavior_id}\""));
        stateforward_hsm::behavior_context::request_abort();
        return None;
    };

    for op in program {
        let Ok(op_object) = object(&op) else {
            inst.fail(format!(
                "timer behavior \"{behavior_id}\" contains a non-object op"
            ));
            stateforward_hsm::behavior_context::request_abort();
            return None;
        };
        let Ok(op_name) = required_string(op_object, "op") else {
            inst.fail(format!(
                "timer behavior \"{behavior_id}\" has an op without a name"
            ));
            stateforward_hsm::behavior_context::request_abort();
            return None;
        };

        match op_name.as_str() {
            "snapshot" => {
                let id = optional_string(op_object, "id")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "last".to_string());
                if let Err(error) = record_context_snapshot(ctx, inst, &id) {
                    inst.fail(error.to_string());
                    stateforward_hsm::behavior_context::request_abort();
                    return None;
                }
            }
            "dispatch" | "raise" => {
                if op_name == "raise" {
                    if let Some(code) = raise_code(op_object) {
                        inst.error(code, issue_message(op_object.get("value")));
                        stateforward_hsm::behavior_context::request_abort();
                        return None;
                    }
                }
                let Some(event) = inst.behavior_event(behavior_id, op_object, &op_name) else {
                    stateforward_hsm::behavior_context::request_abort();
                    return None;
                };
                let target = if op_name == "dispatch" {
                    let Some(target) = inst.behavior_dispatch_target(behavior_id, op_object) else {
                        stateforward_hsm::behavior_context::request_abort();
                        return None;
                    };
                    target
                } else {
                    BehaviorDispatchTarget::Current
                };
                inst.trace_behavior_event(&op_name, &event, target.trace_label());
                inst.dispatch_behavior_event_blocking(ctx, event, target, behavior_id);
                if inst.has_issue() {
                    return None;
                }
            }
            "call" => {
                let Ok(name) = required_string(op_object, "name") else {
                    inst.fail(format!(
                        "timer behavior \"{behavior_id}\" call missing name"
                    ));
                    stateforward_hsm::behavior_context::request_abort();
                    return None;
                };
                if !inst.operation_aliases.contains_key(&name) {
                    inst.error("operation_error".to_string(), name);
                    stateforward_hsm::behavior_context::request_abort();
                    return None;
                }
                let qualified_name = qualify_operation_from_context(ctx, &name);
                inst.run_behavior(ctx, event, &qualified_name);
                if inst.has_issue() {
                    return None;
                }
                inst.dispatch_behavior_event_blocking(
                    ctx,
                    Event::call(qualified_name),
                    BehaviorDispatchTarget::Current,
                    behavior_id,
                );
            }
            "trace" => {
                let value = op_object.get("value").cloned().unwrap_or(Json::Null);
                inst.push_trace(Json::object(vec![
                    ("type", Json::String("trace".to_string())),
                    ("value", value),
                ]));
            }
            "return_value" => {
                let Some(value) = op_object.get("value") else {
                    inst.fail(format!(
                        "timer behavior \"{behavior_id}\" return_value missing value"
                    ));
                    stateforward_hsm::behavior_context::request_abort();
                    return None;
                };
                return match json_u64(value, "return_value") {
                    Ok(value) => Some(value),
                    Err(error) => {
                        inst.error("timer_error".to_string(), error.to_string());
                        stateforward_hsm::behavior_context::request_abort();
                        None
                    }
                };
            }
            "return_attr" => {
                let Ok(name) = required_string(op_object, "name") else {
                    inst.fail(format!(
                        "timer behavior \"{behavior_id}\" return_attr missing name"
                    ));
                    stateforward_hsm::behavior_context::request_abort();
                    return None;
                };
                return timer_attribute_millis(ctx, inst, event, &name);
            }
            other => {
                inst.skip(format!("unsupported timer behavior op \"{other}\""));
                stateforward_hsm::behavior_context::request_abort();
                return None;
            }
        }
    }

    inst.error(
        "timer_error".to_string(),
        format!("timer behavior \"{behavior_id}\" did not return a value"),
    );
    stateforward_hsm::behavior_context::request_abort();
    None
}

fn timer_attribute_millis(
    ctx: &Context,
    inst: &ConformanceInstance,
    event: &Event,
    name: &str,
) -> Option<u64> {
    let (hsm, ok) = FromContext::<ConformanceInstance>(ctx);
    let Some(hsm) = hsm.filter(|_| ok) else {
        inst.error(
            "timer_error".to_string(),
            "timer context missing HSM".to_string(),
        );
        stateforward_hsm::behavior_context::request_abort();
        return None;
    };
    for candidate in timer_attribute_candidates(event, name) {
        if let Some(value) = hsm.get(&candidate) {
            return attribute_value_millis(value, inst);
        }
    }
    inst.error(
        "timer_error".to_string(),
        format!("missing timer attribute \"{name}\""),
    );
    stateforward_hsm::behavior_context::request_abort();
    None
}

fn timer_attribute_candidates(event: &Event, name: &str) -> Vec<String> {
    if name.starts_with('/') {
        return vec![name.to_string()];
    }
    let transition = stateforward_hsm::path::dirname(&event.name).to_string();
    let owner = stateforward_hsm::path::dirname(&transition).to_string();
    let root = event
        .name
        .trim_start_matches('/')
        .split('/')
        .next()
        .map(|root| format!("/{root}"))
        .unwrap_or_else(|| "/".to_string());
    vec![
        stateforward_hsm::path::join(&owner, name),
        stateforward_hsm::path::join(&root, name),
        name.to_string(),
    ]
}

fn attribute_value_millis(value: AttributeValue, inst: &ConformanceInstance) -> Option<u64> {
    match value {
        AttributeValue::Int(value) if value >= 0 => Some(value as u64),
        AttributeValue::String(value) => match value.parse::<u64>() {
            Ok(value) => Some(value),
            Err(_) => {
                inst.error("timer_error".to_string(), "invalid interval".to_string());
                stateforward_hsm::behavior_context::request_abort();
                None
            }
        },
        _ => {
            inst.error("timer_error".to_string(), "invalid interval".to_string());
            stateforward_hsm::behavior_context::request_abort();
            None
        }
    }
}

#[derive(Clone)]
struct PendingGroupDispatch {
    ctx: Context,
    group_id: String,
    event: Event,
}

#[derive(Clone)]
struct ConformanceEventData {
    payload: Option<Json>,
    id: Option<String>,
    source: Option<String>,
    target: Option<String>,
    metadata: Arc<Mutex<BTreeMap<String, Json>>>,
}

impl ConformanceEventData {
    fn new(
        payload: Option<Json>,
        id: Option<String>,
        source: Option<String>,
        target: Option<String>,
        metadata: BTreeMap<String, Json>,
    ) -> Self {
        Self {
            payload,
            id,
            source,
            target,
            metadata: Arc::new(Mutex::new(metadata)),
        }
    }

    fn with_route(&self, source: Option<&str>, target: Option<&str>) -> Self {
        Self {
            payload: self.payload.clone(),
            id: self.id.clone(),
            source: self.source.clone().or_else(|| source.map(str::to_string)),
            target: self.target.clone().or_else(|| target.map(str::to_string)),
            metadata: self.metadata.clone(),
        }
    }
}

#[derive(Clone)]
struct DeferredTraceEvent {
    instance_id: String,
    event_name: String,
    cleanup_on_parent_exit: bool,
}

#[derive(Default)]
struct DeferTraceState {
    deferred_events: Vec<DeferredTraceEvent>,
    defer_replay_barrier: bool,
    trace_defer: bool,
    trace_undefer: bool,
    models: BTreeMap<String, Model<ConformanceInstance>>,
}

#[derive(Clone)]
struct ConformanceInstance {
    behaviors: Arc<HashMap<String, Vec<Json>>>,
    operation_aliases: Arc<HashMap<String, String>>,
    combined_guards: Arc<HashMap<String, (String, String)>>,
    trace_call_behaviors: Arc<BTreeSet<String>>,
    groups: Arc<Mutex<BTreeMap<String, Group<ConformanceInstance>>>>,
    pending_group_dispatches: Arc<Mutex<VecDeque<PendingGroupDispatch>>>,
    trace: Arc<Mutex<Vec<Json>>>,
    defer_trace: Arc<Mutex<DeferTraceState>>,
    snapshots: Arc<Mutex<BTreeMap<String, Json>>>,
    issue: Arc<Mutex<Option<RuntimeIssue>>>,
}

impl ConformanceInstance {
    fn new_with_trace(
        behaviors: HashMap<String, Vec<Json>>,
        operation_aliases: HashMap<String, String>,
        combined_guards: HashMap<String, (String, String)>,
        trace_call_behaviors: BTreeSet<String>,
        groups: Arc<Mutex<BTreeMap<String, Group<ConformanceInstance>>>>,
        pending_group_dispatches: Arc<Mutex<VecDeque<PendingGroupDispatch>>>,
        trace: Arc<Mutex<Vec<Json>>>,
        defer_trace: Arc<Mutex<DeferTraceState>>,
    ) -> Self {
        Self {
            behaviors: Arc::new(behaviors),
            operation_aliases: Arc::new(operation_aliases),
            combined_guards: Arc::new(combined_guards),
            trace_call_behaviors: Arc::new(trace_call_behaviors),
            groups,
            pending_group_dispatches,
            trace,
            defer_trace,
            snapshots: Arc::new(Mutex::new(BTreeMap::new())),
            issue: Arc::new(Mutex::new(None)),
        }
    }

    fn run_behavior(&self, ctx: &Context, event: &Event, operation_name: &str) {
        let operation_id = basename(operation_name);
        let behavior_id = self.behavior_id_for_operation(&operation_id);
        let Some(program) = self.behaviors.get(&behavior_id).cloned() else {
            self.fail(format!("missing behavior \"{behavior_id}\""));
            return;
        };

        for op in program {
            let Ok(op_object) = object(&op) else {
                self.fail(format!(
                    "behavior \"{behavior_id}\" contains a non-object op"
                ));
                return;
            };
            let Ok(op_name) = required_string(op_object, "op") else {
                self.fail(format!(
                    "behavior \"{behavior_id}\" has an op without a name"
                ));
                return;
            };

            match op_name.as_str() {
                "snapshot" => {
                    let id = optional_string(op_object, "id")
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "last".to_string());
                    if let Err(error) = record_context_snapshot(ctx, self, &id) {
                        self.fail(error.to_string());
                        return;
                    }
                }
                "call" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!("behavior \"{behavior_id}\" call missing name"));
                        return;
                    };
                    if !self.operation_aliases.contains_key(&name) {
                        self.error("operation_error".to_string(), name);
                        stateforward_hsm::behavior_context::request_abort();
                        return;
                    }
                    let qualified_name = qualify_operation_from_caller(operation_name, &name);
                    self.run_behavior(ctx, event, &qualified_name);
                    if let Some(issue) = self.take_issue() {
                        *self.issue.lock().unwrap() = Some(issue);
                        return;
                    }
                    if self.trace_call_behaviors.contains(&behavior_id) {
                        self.push_trace(Json::object(vec![
                            ("type", Json::String("call".to_string())),
                            ("operation", Json::String(name.clone())),
                        ]));
                    }
                    self.dispatch_behavior_event_blocking(
                        ctx,
                        Event::call(qualified_name),
                        BehaviorDispatchTarget::Current,
                        &behavior_id,
                    );
                }
                "event_data_equals" => {
                    let Ok(path) = required_string(op_object, "path") else {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" event_data_equals missing path"
                        ));
                        return;
                    };
                    let Some(expected) = op_object.get("value") else {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" event_data_equals missing value"
                        ));
                        return;
                    };
                    if event_json_value(event, &path).as_ref() != Some(expected) {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" event_data_equals mismatch"
                        ));
                        return;
                    }
                }
                "event_metadata_equals" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" event_metadata_equals missing name"
                        ));
                        return;
                    };
                    let Some(expected) = op_object.get("value") else {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" event_metadata_equals missing value"
                        ));
                        return;
                    };
                    if event_metadata_value(event, &name).as_ref() != Some(expected) {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" event_metadata_equals mismatch"
                        ));
                        return;
                    }
                }
                "event_metadata_get" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" event_metadata_get missing name"
                        ));
                        return;
                    };
                    let matched = event_metadata_value(event, &name)
                        .as_ref()
                        .map_or(false, json_truthy);
                    if !matched {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" event_metadata_get mismatch"
                        ));
                        return;
                    }
                }
                "event_metadata_set" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" event_metadata_set missing name"
                        ));
                        return;
                    };
                    let Some(value) = op_object.get("value") else {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" event_metadata_set missing value"
                        ));
                        return;
                    };
                    set_event_metadata(event, &name, value.clone());
                }
                "event_name_equals" => {
                    let Ok(expected) = required_string(op_object, "value") else {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" event_name_equals missing value"
                        ));
                        return;
                    };
                    if event.name != expected {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" event_name_equals mismatch"
                        ));
                        return;
                    }
                }
                "return_equals" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" return_equals missing name"
                        ));
                        return;
                    };
                    let Some(expected) = op_object.get("value") else {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" return_equals missing value"
                        ));
                        return;
                    };
                    let Ok(expected) = json_to_attribute_value(expected) else {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" return_equals unsupported value"
                        ));
                        return;
                    };
                    let _ = GetFromContext(ctx, &name).as_ref() == Some(&expected);
                }
                "set_attr" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!("behavior \"{behavior_id}\" set_attr missing name"));
                        return;
                    };
                    let Some(value) = op_object.get("value") else {
                        self.fail(format!("behavior \"{behavior_id}\" set_attr missing value"));
                        return;
                    };
                    let value = match json_to_attribute_value(value) {
                        Ok(value) => value,
                        Err(error) => {
                            self.fail(error.to_string());
                            return;
                        }
                    };
                    if let Err(error) = SetFromContext(ctx, &name, value) {
                        self.error("attribute_error".to_string(), error.to_string());
                        stateforward_hsm::behavior_context::request_abort();
                        return;
                    }
                }
                "set_attr_from_event_data" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" set_attr_from_event_data missing name"
                        ));
                        return;
                    };
                    let Ok(path) = required_string(op_object, "path") else {
                        self.fail(format!(
                            "behavior \"{behavior_id}\" set_attr_from_event_data missing path"
                        ));
                        return;
                    };
                    let value = match event_json_value(event, &path) {
                        Some(value) => match json_to_attribute_value(&value) {
                            Ok(value) => value,
                            Err(error) => {
                                self.fail(error.to_string());
                                return;
                            }
                        },
                        None => AttributeValue::Null,
                    };
                    if let Err(error) = SetFromContext(ctx, &name, value) {
                        self.error("attribute_error".to_string(), error.to_string());
                        stateforward_hsm::behavior_context::request_abort();
                        return;
                    }
                }
                "dispatch" | "raise" => {
                    if op_name == "raise" {
                        if let Some(code) = raise_code(op_object) {
                            self.error(code, issue_message(op_object.get("value")));
                            stateforward_hsm::behavior_context::request_abort();
                            return;
                        }
                    }
                    let Some(event) = self.behavior_event(&behavior_id, op_object, &op_name) else {
                        return;
                    };
                    let target = if op_name == "dispatch" {
                        let Some(target) = self.behavior_dispatch_target(&behavior_id, op_object)
                        else {
                            return;
                        };
                        target
                    } else {
                        BehaviorDispatchTarget::Current
                    };
                    self.trace_behavior_event(&op_name, &event, target.trace_label());
                    self.dispatch_behavior_event_blocking(ctx, event, target, &behavior_id);
                }
                "trace" => {
                    let value = op_object.get("value").cloned().unwrap_or(Json::Null);
                    self.trace_undefer_before_behavior_trace(ctx);
                    self.push_trace(Json::object(vec![
                        ("type", Json::String("trace".to_string())),
                        ("value", value),
                    ]));
                }
                other => {
                    self.skip(format!("unsupported behavior op \"{other}\""));
                    return;
                }
            }
        }
    }

    fn run_behavior_async(
        &self,
        ctx: Context,
        event: Event,
        operation_name: String,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let instance = self.clone();
        Box::pin(async move {
            let operation_id = basename(&operation_name);
            let behavior_id = instance.behavior_id_for_operation(&operation_id);
            let Some(program) = instance.behaviors.get(&behavior_id).cloned() else {
                instance.fail(format!("missing behavior \"{behavior_id}\""));
                return;
            };
            for op in program {
                let Ok(op_object) = object(&op) else {
                    instance.fail(format!(
                        "behavior \"{behavior_id}\" contains a non-object op"
                    ));
                    return;
                };
                let Ok(op_name) = required_string(op_object, "op") else {
                    instance.fail(format!(
                        "behavior \"{behavior_id}\" has an op without a name"
                    ));
                    return;
                };

                match op_name.as_str() {
                    "yield" => {
                        let activity = stateforward_hsm::behavior_context::current_activity(&ctx);
                        if activity.as_deref() == Some(behavior_id.as_str()) {
                            stateforward_hsm::behavior_context::end_activity(&ctx);
                        }
                        tokio::task::yield_now().await;
                        if ctx.is_cancelled() {
                            return;
                        }
                        if let Some(activity) = activity {
                            stateforward_hsm::behavior_context::begin_activity(&ctx, &activity);
                        }
                    }
                    "sleep" => {
                        let millis = match optional_number_u64(op_object, "millis") {
                            Ok(Some(millis)) => millis,
                            Ok(None) => {
                                instance.fail(format!(
                                    "behavior \"{behavior_id}\" sleep missing millis"
                                ));
                                return;
                            }
                            Err(error) => {
                                instance.fail(error.to_string());
                                return;
                            }
                        };
                        let Some(clock) = current_clock_handle() else {
                            instance.fail("sleep requires a logical clock".to_string());
                            return;
                        };
                        let instance_id =
                            current_instance_id(&ctx).unwrap_or_else(|| "default".to_string());
                        let sleep = stateforward_hsm::behavior_context::with_timer_registration(
                            &instance_id,
                            "",
                            "",
                            &ctx,
                            || {
                                clock.sleep(
                                    Duration::from_millis(millis),
                                    None,
                                    instance.clone(),
                                    false,
                                    false,
                                )
                            },
                        );
                        let activity = stateforward_hsm::behavior_context::current_activity(&ctx);
                        if activity.as_deref() == Some(behavior_id.as_str()) {
                            stateforward_hsm::behavior_context::end_activity(&ctx);
                        }
                        sleep.await;
                        if ctx.is_cancelled() {
                            return;
                        }
                        if let Some(activity) = activity {
                            stateforward_hsm::behavior_context::begin_activity(&ctx, &activity);
                        }
                    }
                    "snapshot" => {
                        let id = optional_string(op_object, "id")
                            .ok()
                            .flatten()
                            .unwrap_or_else(|| "last".to_string());
                        if let Err(error) = record_context_snapshot(&ctx, &instance, &id) {
                            instance.fail(error.to_string());
                            return;
                        }
                    }
                    "call" => {
                        let Ok(name) = required_string(op_object, "name") else {
                            instance.fail(format!("behavior \"{behavior_id}\" call missing name"));
                            return;
                        };
                        if !instance.operation_aliases.contains_key(&name) {
                            instance.error("operation_error".to_string(), name);
                            stateforward_hsm::behavior_context::request_abort();
                            return;
                        }
                        let qualified_name = qualify_operation_from_caller(&operation_name, &name);
                        instance
                            .run_behavior_async(ctx.clone(), event.clone(), qualified_name.clone())
                            .await;
                        if instance.has_issue() {
                            return;
                        }
                        if instance.trace_call_behaviors.contains(&behavior_id) {
                            instance.push_trace(Json::object(vec![
                                ("type", Json::String("call".to_string())),
                                ("operation", Json::String(name.clone())),
                            ]));
                        }
                        instance
                            .dispatch_behavior_event_async(
                                &ctx,
                                Event::call(qualified_name),
                                BehaviorDispatchTarget::Current,
                                &behavior_id,
                            )
                            .await;
                    }
                    "dispatch" | "raise" => {
                        if op_name == "raise" {
                            if let Some(code) = raise_code(op_object) {
                                instance.error(code, issue_message(op_object.get("value")));
                                stateforward_hsm::behavior_context::request_abort();
                                return;
                            }
                        }
                        let Some(event) =
                            instance.behavior_event(&behavior_id, op_object, &op_name)
                        else {
                            return;
                        };
                        let target = if op_name == "dispatch" {
                            let Some(target) =
                                instance.behavior_dispatch_target(&behavior_id, op_object)
                            else {
                                return;
                            };
                            target
                        } else {
                            BehaviorDispatchTarget::Current
                        };
                        instance.trace_behavior_event(&op_name, &event, target.trace_label());
                        instance
                            .dispatch_behavior_event_async(&ctx, event, target, &behavior_id)
                            .await;
                    }
                    "event_data_equals" => {
                        let Ok(path) = required_string(op_object, "path") else {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" event_data_equals missing path"
                            ));
                            return;
                        };
                        let Some(expected) = op_object.get("value") else {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" event_data_equals missing value"
                            ));
                            return;
                        };
                        if event_json_value(&event, &path).as_ref() != Some(expected) {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" event_data_equals mismatch"
                            ));
                            return;
                        }
                    }
                    "event_metadata_equals" => {
                        let Ok(name) = required_string(op_object, "name") else {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" event_metadata_equals missing name"
                            ));
                            return;
                        };
                        let Some(expected) = op_object.get("value") else {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" event_metadata_equals missing value"
                            ));
                            return;
                        };
                        if event_metadata_value(&event, &name).as_ref() != Some(expected) {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" event_metadata_equals mismatch"
                            ));
                            return;
                        }
                    }
                    "event_metadata_get" => {
                        let Ok(name) = required_string(op_object, "name") else {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" event_metadata_get missing name"
                            ));
                            return;
                        };
                        let matched = event_metadata_value(&event, &name)
                            .as_ref()
                            .map_or(false, json_truthy);
                        if !matched {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" event_metadata_get mismatch"
                            ));
                            return;
                        }
                    }
                    "event_metadata_set" => {
                        let Ok(name) = required_string(op_object, "name") else {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" event_metadata_set missing name"
                            ));
                            return;
                        };
                        let Some(value) = op_object.get("value") else {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" event_metadata_set missing value"
                            ));
                            return;
                        };
                        set_event_metadata(&event, &name, value.clone());
                    }
                    "event_name_equals" => {
                        let Ok(expected) = required_string(op_object, "value") else {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" event_name_equals missing value"
                            ));
                            return;
                        };
                        if event.name != expected {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" event_name_equals mismatch"
                            ));
                            return;
                        }
                    }
                    "return_equals" => {
                        let Ok(name) = required_string(op_object, "name") else {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" return_equals missing name"
                            ));
                            return;
                        };
                        let Some(expected) = op_object.get("value") else {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" return_equals missing value"
                            ));
                            return;
                        };
                        let Ok(expected) = json_to_attribute_value(expected) else {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" return_equals unsupported value"
                            ));
                            return;
                        };
                        let _ = GetFromContext(&ctx, &name).as_ref() == Some(&expected);
                    }
                    "set_attr" => {
                        let Ok(name) = required_string(op_object, "name") else {
                            instance
                                .fail(format!("behavior \"{behavior_id}\" set_attr missing name"));
                            return;
                        };
                        let Some(value) = op_object.get("value") else {
                            instance
                                .fail(format!("behavior \"{behavior_id}\" set_attr missing value"));
                            return;
                        };
                        let value = match json_to_attribute_value(value) {
                            Ok(value) => value,
                            Err(error) => {
                                instance.fail(error.to_string());
                                return;
                            }
                        };
                        if let Err(error) = SetFromContext(&ctx, &name, value) {
                            instance.error("attribute_error".to_string(), error.to_string());
                            stateforward_hsm::behavior_context::request_abort();
                            return;
                        }
                    }
                    "set_attr_from_event_data" => {
                        let Ok(name) = required_string(op_object, "name") else {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" set_attr_from_event_data missing name"
                            ));
                            return;
                        };
                        let Ok(path) = required_string(op_object, "path") else {
                            instance.fail(format!(
                                "behavior \"{behavior_id}\" set_attr_from_event_data missing path"
                            ));
                            return;
                        };
                        let value = match event_json_value(&event, &path) {
                            Some(value) => match json_to_attribute_value(&value) {
                                Ok(value) => value,
                                Err(error) => {
                                    instance.fail(error.to_string());
                                    return;
                                }
                            },
                            None => AttributeValue::Null,
                        };
                        if let Err(error) = SetFromContext(&ctx, &name, value) {
                            instance.error("attribute_error".to_string(), error.to_string());
                            stateforward_hsm::behavior_context::request_abort();
                            return;
                        }
                    }
                    "trace" => {
                        let value = op_object.get("value").cloned().unwrap_or(Json::Null);
                        instance.trace_undefer_before_behavior_trace(&ctx);
                        instance.push_trace(Json::object(vec![
                            ("type", Json::String("trace".to_string())),
                            ("value", value),
                        ]));
                    }
                    other => {
                        instance.skip(format!("unsupported behavior op \"{other}\""));
                        return;
                    }
                }
                if ctx.is_cancelled()
                    && stateforward_hsm::behavior_context::current_activity(&ctx).as_deref()
                        == Some(behavior_id.as_str())
                {
                    return;
                }
            }
        })
    }

    fn run_guard_behavior(&self, ctx: &Context, event: &Event, operation_name: &str) -> bool {
        let operation_id = basename(operation_name);
        if let Some((when_behavior, guard_behavior)) = self.combined_guards.get(&operation_id) {
            return self.run_guard_behavior_id(ctx, event, when_behavior, operation_name)
                && self.run_guard_behavior_id(ctx, event, guard_behavior, operation_name);
        }
        let behavior_id = self.behavior_id_for_operation(&operation_id);
        self.run_guard_behavior_id(ctx, event, &behavior_id, operation_name)
    }

    fn run_guard_behavior_id(
        &self,
        ctx: &Context,
        event: &Event,
        behavior_id: &str,
        operation_name: &str,
    ) -> bool {
        let Some(program) = self.behaviors.get(behavior_id).cloned() else {
            self.fail(format!("missing guard behavior \"{behavior_id}\""));
            return false;
        };
        let trace_start = self.trace.lock().unwrap().len();
        let mut result = None;

        for op in program {
            let Ok(op_object) = object(&op) else {
                self.fail(format!(
                    "guard behavior \"{behavior_id}\" contains a non-object op"
                ));
                return false;
            };
            let Ok(op_name) = required_string(op_object, "op") else {
                self.fail(format!(
                    "guard behavior \"{behavior_id}\" has an op without a name"
                ));
                return false;
            };

            match op_name.as_str() {
                "snapshot" => {
                    let id = optional_string(op_object, "id")
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "last".to_string());
                    if let Err(error) = record_context_snapshot(ctx, self, &id) {
                        self.fail(error.to_string());
                        return false;
                    }
                }
                "trace" => {
                    let value = op_object.get("value").cloned().unwrap_or(Json::Null);
                    self.trace_undefer_before_behavior_trace(ctx);
                    self.push_trace(Json::object(vec![
                        ("type", Json::String("trace".to_string())),
                        ("value", value),
                    ]));
                }
                "return_value" => {
                    let accepted = matches!(op_object.get("value"), Some(Json::Bool(true)));
                    if accepted {
                        self.move_timer_fired_after_guard_trace(event, trace_start);
                    }
                    return accepted;
                }
                "event_data_get" => {
                    let Ok(path) = required_string(op_object, "path") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" event_data_get missing path"
                        ));
                        return false;
                    };
                    let matched = event_json_value(event, &path)
                        .as_ref()
                        .map_or(false, json_truthy);
                    if !matched {
                        return false;
                    }
                    result = Some(true);
                }
                "event_data_equals" => {
                    let Ok(path) = required_string(op_object, "path") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" event_data_equals missing path"
                        ));
                        return false;
                    };
                    let Some(expected) = op_object.get("value") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" event_data_equals missing value"
                        ));
                        return false;
                    };
                    if event_json_value(event, &path).as_ref() != Some(expected) {
                        return false;
                    }
                    result = Some(true);
                }
                "event_metadata_get" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" event_metadata_get missing name"
                        ));
                        return false;
                    };
                    let matched = event_metadata_value(event, &name)
                        .as_ref()
                        .map_or(false, json_truthy);
                    if !matched {
                        return false;
                    }
                    result = Some(true);
                }
                "event_metadata_equals" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" event_metadata_equals missing name"
                        ));
                        return false;
                    };
                    let Some(expected) = op_object.get("value") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" event_metadata_equals missing value"
                        ));
                        return false;
                    };
                    if event_metadata_value(event, &name).as_ref() != Some(expected) {
                        return false;
                    }
                    result = Some(true);
                }
                "event_application_metadata_equals" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" event_application_metadata_equals missing name"
                        ));
                        return false;
                    };
                    let Some(expected) = op_object.get("value") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" event_application_metadata_equals missing value"
                        ));
                        return false;
                    };
                    if event_application_metadata_value(event, &name).as_ref() != Some(expected) {
                        return false;
                    }
                    result = Some(true);
                }
                "event_metadata_set" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" event_metadata_set missing name"
                        ));
                        return false;
                    };
                    let Some(value) = op_object.get("value") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" event_metadata_set missing value"
                        ));
                        return false;
                    };
                    set_event_metadata(event, &name, value.clone());
                }
                "event_name_equals" => {
                    let Ok(expected) = required_string(op_object, "value") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" event_name_equals missing value"
                        ));
                        return false;
                    };
                    if event.name != expected {
                        return false;
                    }
                    result = Some(true);
                }
                "get_attr" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" get_attr missing name"
                        ));
                        return false;
                    };
                    let Some(value) = GetFromContext(ctx, &name) else {
                        return false;
                    };
                    if !attribute_truthy(&value) {
                        return false;
                    }
                    result = Some(true);
                }
                "call" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" call missing name"
                        ));
                        return false;
                    };
                    if !self.operation_aliases.contains_key(&name) {
                        self.error("operation_error".to_string(), name);
                        stateforward_hsm::behavior_context::request_abort();
                        return false;
                    }
                    let qualified_name = qualify_operation_from_caller(operation_name, &name);
                    self.run_behavior(ctx, event, &qualified_name);
                    if let Some(issue) = self.take_issue() {
                        *self.issue.lock().unwrap() = Some(issue);
                        return false;
                    }
                    self.dispatch_behavior_event_blocking(
                        ctx,
                        Event::call(qualified_name),
                        BehaviorDispatchTarget::Current,
                        behavior_id,
                    );
                }
                "return_equals" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" return_equals missing name"
                        ));
                        return false;
                    };
                    let Some(expected) = op_object.get("value") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" return_equals missing value"
                        ));
                        return false;
                    };
                    let Ok(expected) = json_to_attribute_value(expected) else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" return_equals unsupported value"
                        ));
                        return false;
                    };
                    let accepted = GetFromContext(ctx, &name).as_ref() == Some(&expected);
                    if accepted {
                        self.move_timer_fired_after_guard_trace(event, trace_start);
                    }
                    return accepted;
                }
                "return_attr" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" return_attr missing name"
                        ));
                        return false;
                    };
                    let accepted =
                        GetFromContext(ctx, &name).is_some_and(|value| attribute_truthy(&value));
                    if accepted {
                        self.move_timer_fired_after_guard_trace(event, trace_start);
                    }
                    return accepted;
                }
                "set_attr" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" set_attr missing name"
                        ));
                        return false;
                    };
                    let Some(value) = op_object.get("value") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" set_attr missing value"
                        ));
                        return false;
                    };
                    let Ok(value) = json_to_attribute_value(value) else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" set_attr unsupported value"
                        ));
                        return false;
                    };
                    if let Err(error) = SetFromContext(ctx, &name, value) {
                        self.error("attribute_error".to_string(), error.to_string());
                        stateforward_hsm::behavior_context::request_abort();
                        return false;
                    }
                }
                "set_attr_from_event_data" => {
                    let Ok(name) = required_string(op_object, "name") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" set_attr_from_event_data missing name"
                        ));
                        return false;
                    };
                    let Ok(path) = required_string(op_object, "path") else {
                        self.fail(format!(
                            "guard behavior \"{behavior_id}\" set_attr_from_event_data missing path"
                        ));
                        return false;
                    };
                    let value = match event_json_value(event, &path) {
                        Some(value) => match json_to_attribute_value(&value) {
                            Ok(value) => value,
                            Err(error) => {
                                self.fail(error.to_string());
                                return false;
                            }
                        },
                        None => AttributeValue::Null,
                    };
                    if let Err(error) = SetFromContext(ctx, &name, value) {
                        self.error("attribute_error".to_string(), error.to_string());
                        stateforward_hsm::behavior_context::request_abort();
                        return false;
                    }
                }
                "dispatch" | "raise" => {
                    if op_name == "raise" {
                        if let Some(code) = raise_code(op_object) {
                            self.error(code, issue_message(op_object.get("value")));
                            stateforward_hsm::behavior_context::request_abort();
                            return false;
                        }
                    }
                    let Some(event) = self.behavior_event(behavior_id, op_object, &op_name) else {
                        return false;
                    };
                    let target = if op_name == "dispatch" {
                        let Some(target) = self.behavior_dispatch_target(behavior_id, op_object)
                        else {
                            return false;
                        };
                        target
                    } else {
                        BehaviorDispatchTarget::Current
                    };
                    self.trace_behavior_event(&op_name, &event, target.trace_label());
                    self.dispatch_behavior_event_blocking(ctx, event, target, behavior_id);
                }
                other => {
                    self.skip(format!("unsupported guard behavior op \"{other}\""));
                    return false;
                }
            }
        }

        let accepted = result.unwrap_or(false);
        if accepted {
            self.move_timer_fired_after_guard_trace(event, trace_start);
        }
        accepted
    }

    fn move_timer_fired_after_guard_trace(&self, event: &Event, trace_start: usize) {
        if trace_start == 0 || !is_registered_timer_event(&event.name) {
            return;
        }
        let mut trace = self.trace.lock().unwrap();
        let fired_index = trace_start - 1;
        if !trace_entry_is_type(&trace[fired_index], "timer_fired") {
            return;
        }
        if fired_index + 1 >= trace.len() {
            return;
        }
        let entry = trace.remove(fired_index);
        trace.push(entry);
    }

    fn fail(&self, message: String) {
        *self.issue.lock().unwrap() = Some(RuntimeIssue {
            skip: false,
            code: None,
            message,
        });
    }

    fn skip(&self, message: String) {
        *self.issue.lock().unwrap() = Some(RuntimeIssue {
            skip: true,
            code: None,
            message,
        });
    }

    fn error(&self, code: String, message: String) {
        self.push_trace(Json::object(vec![
            ("type", Json::String("error".to_string())),
            ("code", Json::String(code.clone())),
        ]));
        *self.issue.lock().unwrap() = Some(RuntimeIssue {
            skip: false,
            code: Some(code),
            message,
        });
    }

    fn push_trace(&self, entry: Json) {
        self.trace.lock().unwrap().push(entry);
    }

    fn trace_undefer_before_behavior_trace(&self, ctx: &Context) {
        let Some(instance_id) = current_instance_id(ctx) else {
            return;
        };
        let event_name = {
            let mut defer_trace = self.defer_trace.lock().unwrap();
            if !defer_trace.trace_undefer || defer_trace.deferred_events.is_empty() {
                return;
            }
            if defer_trace.defer_replay_barrier {
                defer_trace.defer_replay_barrier = false;
                return;
            }
            pop_deferred_event_for_instance(&mut defer_trace, &instance_id)
        };
        if let Some(event_name) = event_name {
            self.push_trace(Json::object(vec![
                ("type", Json::String("undefer".to_string())),
                ("event", Json::String(event_name)),
            ]));
        }
    }

    fn trace_deferred_dispatch_records(
        &self,
        event_name: &str,
        records: &[(String, String)],
    ) -> BTreeSet<String> {
        let mut traced = BTreeSet::new();
        let mut trace_count = 0;
        {
            let mut defer_trace = self.defer_trace.lock().unwrap();
            if !defer_trace.trace_defer {
                return traced;
            }
            for (instance_id, state) in records {
                if event_is_deferred(&defer_trace, instance_id, state, event_name).is_some()
                    && !event_has_transition_candidate(&defer_trace, instance_id, state, event_name)
                    && !has_deferred_event(&defer_trace, instance_id, event_name)
                {
                    note_deferred_event(&mut defer_trace, instance_id, state, event_name);
                    traced.insert(instance_id.clone());
                    trace_count += 1;
                }
            }
        }
        for _ in 0..trace_count {
            self.push_trace(Json::object(vec![
                ("type", Json::String("defer".to_string())),
                ("event", Json::String(event_name.to_string())),
            ]));
        }
        traced
    }

    fn trace_runtime_deferred_records(
        &self,
        event_name: &str,
        records: &[(String, String)],
        traced: &BTreeSet<String>,
    ) {
        let mut trace_count = 0;
        {
            let mut defer_trace = self.defer_trace.lock().unwrap();
            if !defer_trace.trace_defer {
                return;
            }
            for (instance_id, state) in records {
                if traced.contains(instance_id) {
                    continue;
                }
                if event_is_deferred(&defer_trace, instance_id, state, event_name).is_some()
                    && !has_deferred_event(&defer_trace, instance_id, event_name)
                {
                    note_deferred_event(&mut defer_trace, instance_id, state, event_name);
                    trace_count += 1;
                }
            }
        }
        for _ in 0..trace_count {
            self.push_trace(Json::object(vec![
                ("type", Json::String("defer".to_string())),
                ("event", Json::String(event_name.to_string())),
            ]));
        }
    }

    fn trace_deferred_behavior_owner_event(
        &self,
        ctx: &Context,
        behavior_id: &str,
        event_name: &str,
    ) -> BTreeSet<String> {
        let mut traced = BTreeSet::new();
        let Some(instance_id) = current_instance_id(ctx) else {
            return traced;
        };

        let mut trace_count = 0;
        {
            let mut defer_trace = self.defer_trace.lock().unwrap();
            if !defer_trace.trace_defer
                || has_deferred_event(&defer_trace, &instance_id, event_name)
            {
                return traced;
            }

            let owner = defer_trace
                .models
                .get(&instance_id)
                .and_then(|model| behavior_owner_defer_state(model, behavior_id, event_name));
            let Some(owner) = owner else {
                return traced;
            };

            let cleanup_on_parent_exit = defer_trace
                .models
                .get(&instance_id)
                .map(|model| deferred_cleanup_on_parent_exit(model, &owner))
                .unwrap_or(false);
            defer_trace.deferred_events.push(DeferredTraceEvent {
                instance_id: instance_id.clone(),
                event_name: event_name.to_string(),
                cleanup_on_parent_exit,
            });
            traced.insert(instance_id);
            trace_count += 1;
        }

        for _ in 0..trace_count {
            self.push_trace(Json::object(vec![
                ("type", Json::String("defer".to_string())),
                ("event", Json::String(event_name.to_string())),
            ]));
        }
        traced
    }

    fn trace_undefer_before_dispatch(&self, instance_id: &str, state: &str, event_name: &str) {
        let event_name = {
            let mut defer_trace = self.defer_trace.lock().unwrap();
            let event_deferred_by_current_state =
                event_is_deferred(&defer_trace, instance_id, state, event_name).is_some()
                    && !event_has_transition_candidate(
                        &defer_trace,
                        instance_id,
                        state,
                        event_name,
                    );
            if defer_trace.deferred_events.is_empty() || event_deferred_by_current_state {
                return;
            }
            if event_exits_active_submachine(&defer_trace, instance_id, state, event_name) {
                clear_child_deferred_events_for_instance(&mut defer_trace, instance_id);
            }
            if defer_trace.deferred_events.is_empty() {
                return;
            }
            let event_name = pop_deferred_event_for_instance(&mut defer_trace, instance_id);
            if event_name.is_some() {
                defer_trace.defer_replay_barrier = true;
            }
            event_name
        };
        if let Some(event_name) = event_name {
            self.push_trace(Json::object(vec![
                ("type", Json::String("undefer".to_string())),
                ("event", Json::String(event_name)),
            ]));
        }
    }

    fn behavior_event(
        &self,
        behavior_id: &str,
        op: &BTreeMap<String, Json>,
        op_name: &str,
    ) -> Option<Event> {
        let Some(event_ir) = op.get("event").or_else(|| op.get("value")) else {
            self.fail(format!(
                "behavior \"{behavior_id}\" {op_name} missing event"
            ));
            return None;
        };
        match event_from_json(event_ir) {
            Ok(event) => Some(event),
            Err(error) => {
                self.fail(error.to_string());
                None
            }
        }
    }

    fn behavior_dispatch_target(
        &self,
        behavior_id: &str,
        op: &BTreeMap<String, Json>,
    ) -> Option<BehaviorDispatchTarget> {
        if op.contains_key("target") && op.contains_key("instance") {
            self.fail(format!(
                "behavior \"{behavior_id}\" dispatch cannot declare target and instance"
            ));
            return None;
        }
        if op.contains_key("target") && op.contains_key("group") {
            self.fail(format!(
                "behavior \"{behavior_id}\" dispatch cannot declare target and group"
            ));
            return None;
        }
        if op.contains_key("instance") && op.contains_key("group") {
            self.fail(format!(
                "behavior \"{behavior_id}\" dispatch cannot declare instance and group"
            ));
            return None;
        }

        match optional_string(op, "group") {
            Ok(Some(group)) => return Some(BehaviorDispatchTarget::Group(group)),
            Ok(None) => {}
            Err(error) => {
                self.fail(error.to_string());
                return None;
            }
        }

        let target = match optional_string(op, "instance") {
            Ok(Some(target)) => return Some(BehaviorDispatchTarget::Instance(target)),
            Ok(None) => match optional_string(op, "target") {
                Ok(target) => target,
                Err(error) => {
                    self.fail(error.to_string());
                    return None;
                }
            },
            Err(error) => {
                self.fail(error.to_string());
                return None;
            }
        };
        match target.as_deref() {
            Some("all") => Some(BehaviorDispatchTarget::All),
            Some(target) => Some(BehaviorDispatchTarget::Instance(target.to_string())),
            None => Some(BehaviorDispatchTarget::Current),
        }
    }

    fn trace_behavior_event(&self, op_name: &str, event: &Event, target: Option<&str>) {
        let mut entry = vec![
            ("type", Json::String(op_name.to_string())),
            ("event", Json::String(event.name.clone())),
        ];
        if let Some(target) = target {
            entry.push(("target", Json::String(target.to_string())));
        }
        self.push_trace(Json::object(entry));
    }

    async fn dispatch_behavior_event_async(
        &self,
        ctx: &Context,
        event: Event,
        target: BehaviorDispatchTarget,
        behavior_id: &str,
    ) {
        let event_name = event.name.clone();
        let before_records = behavior_target_records(ctx, &target, &self.groups);
        let mut traced = self.trace_deferred_dispatch_records(&event_name, &before_records);
        if traced.is_empty() && matches!(target, BehaviorDispatchTarget::Current) {
            traced.extend(self.trace_deferred_behavior_owner_event(ctx, behavior_id, &event_name));
        }
        match target.clone() {
            BehaviorDispatchTarget::Current => {
                let _ = DispatchFromContext(ctx, event).await;
            }
            BehaviorDispatchTarget::Instance(target) => {
                let source = current_instance_id(ctx);
                let event = event_with_route(&event, source.as_deref(), Some(&target));
                let _ = DispatchTo(ctx, event, vec![target]).await;
            }
            BehaviorDispatchTarget::All => {
                let _ = dispatch_all_with_route_targets(ctx, event, current_instance_id(ctx)).await;
            }
            BehaviorDispatchTarget::Group(group_id) => {
                let source = current_instance_id(ctx);
                self.queue_pending_group_dispatch(
                    ctx,
                    group_id,
                    event_with_route(&event, source.as_deref(), None),
                );
            }
        }
        let after_records = behavior_target_records(ctx, &target, &self.groups);
        self.trace_runtime_deferred_records(&event_name, &after_records, &traced);
    }

    fn dispatch_behavior_event_blocking(
        &self,
        ctx: &Context,
        event: Event,
        target: BehaviorDispatchTarget,
        behavior_id: &str,
    ) {
        let event_name = event.name.clone();
        let before_records = behavior_target_records(ctx, &target, &self.groups);
        let mut traced = self.trace_deferred_dispatch_records(&event_name, &before_records);
        if traced.is_empty() && matches!(target, BehaviorDispatchTarget::Current) {
            traced.extend(self.trace_deferred_behavior_owner_event(ctx, behavior_id, &event_name));
        }
        let target = match target {
            BehaviorDispatchTarget::Group(group_id) => {
                let source = current_instance_id(ctx);
                let trace_target = BehaviorDispatchTarget::Group(group_id.clone());
                self.queue_pending_group_dispatch(
                    ctx,
                    group_id,
                    event_with_route(&event, source.as_deref(), None),
                );
                let after_records = behavior_target_records(ctx, &trace_target, &self.groups);
                self.trace_runtime_deferred_records(&event_name, &after_records, &traced);
                return;
            }
            target => target,
        };
        let trace_target = target.clone();
        let after_ctx = ctx.clone();
        let ctx = ctx.clone();
        let _ = std::thread::spawn(move || {
            if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                runtime.block_on(async move {
                    match target {
                        BehaviorDispatchTarget::Current => {
                            let _ = DispatchFromContext(&ctx, event).await;
                        }
                        BehaviorDispatchTarget::Instance(target) => {
                            let source = current_instance_id(&ctx);
                            let event = event_with_route(&event, source.as_deref(), Some(&target));
                            let _ = DispatchTo(&ctx, event, vec![target]).await;
                        }
                        BehaviorDispatchTarget::All => {
                            let _ = dispatch_all_with_route_targets(
                                &ctx,
                                event,
                                current_instance_id(&ctx),
                            )
                            .await;
                        }
                        BehaviorDispatchTarget::Group(_) => unreachable!(),
                    }
                });
            }
        })
        .join();
        let after_records = behavior_target_records(&after_ctx, &trace_target, &self.groups);
        self.trace_runtime_deferred_records(&event_name, &after_records, &traced);
    }

    fn queue_pending_group_dispatch(&self, ctx: &Context, group_id: String, event: Event) {
        if !self.groups.lock().unwrap().contains_key(&group_id) {
            self.error(
                "runtime_error".to_string(),
                format!("unknown group \"{group_id}\""),
            );
            stateforward_hsm::behavior_context::request_abort();
            return;
        }
        self.pending_group_dispatches
            .lock()
            .unwrap()
            .push_back(PendingGroupDispatch {
                ctx: ctx.clone(),
                group_id,
                event,
            });
    }

    fn insert_snapshot(&self, id: String, snapshot: Json) {
        self.snapshots.lock().unwrap().insert(id, snapshot);
    }

    fn trace(&self) -> Vec<Json> {
        self.trace.lock().unwrap().clone()
    }

    fn snapshots(&self) -> BTreeMap<String, Json> {
        self.snapshots.lock().unwrap().clone()
    }

    fn take_issue(&self) -> Option<RuntimeIssue> {
        self.issue.lock().unwrap().take()
    }

    fn has_issue(&self) -> bool {
        self.issue.lock().unwrap().is_some()
    }

    fn behavior_id_for_operation(&self, operation_id: &str) -> String {
        self.operation_aliases
            .get(operation_id)
            .cloned()
            .unwrap_or_else(|| operation_id.to_string())
    }
}

impl Instance for ConformanceInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn activity_done(&self, behavior: &str) {
        if !activity_trace_enabled("activity_done") {
            return;
        }
        self.push_trace(Json::object(vec![
            ("type", Json::String("activity_done".to_string())),
            ("behavior", Json::String(behavior.to_string())),
        ]));
    }

    fn activity_cancelled(&self, behavior: &str) {
        if !activity_trace_enabled("activity_cancel") {
            return;
        }
        self.push_trace(Json::object(vec![
            ("type", Json::String("activity_cancel".to_string())),
            ("behavior", Json::String(behavior.to_string())),
        ]));
    }
}

struct CaseData {
    name: String,
    features: Vec<String>,
    mode: String,
    model: Json,
    models: Vec<Json>,
    behaviors: HashMap<String, Vec<Json>>,
    operation_aliases: HashMap<String, String>,
    combined_guards: HashMap<String, (String, String)>,
    instances: Vec<Json>,
    groups: Vec<Json>,
    script: Vec<Json>,
    expect: BTreeMap<String, Json>,
}

enum CaseResult {
    Pass { name: String },
    Skip { name: String, reason: String },
    Fail { name: String, error: String },
}

#[derive(Clone, Copy)]
struct StepTraceOptions {
    set: bool,
    start: bool,
    restart: bool,
    stop: bool,
    timer_cancelled: bool,
}

struct DispatchTargets {
    ids: Vec<String>,
    trace_target: Json,
    stable_label: String,
}

#[derive(Clone, Debug)]
enum BehaviorDispatchTarget {
    Current,
    Instance(String),
    All,
    Group(String),
}

impl BehaviorDispatchTarget {
    fn trace_label(&self) -> Option<&str> {
        match self {
            Self::Current => None,
            Self::Instance(id) | Self::Group(id) => Some(id.as_str()),
            Self::All => Some("all"),
        }
    }
}

fn conformance_operation(
    ctx: &Context,
    inst: &mut ConformanceInstance,
    event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let operation_name =
        stateforward_hsm::behavior_context::current_operation_name().unwrap_or_default();
    let instance = inst.clone();
    let ctx = ctx.clone();
    let event = event.clone();
    instance.run_behavior_async(ctx, event, operation_name)
}

fn conformance_guard(ctx: &Context, inst: &ConformanceInstance, event: &Event) -> bool {
    let operation_name =
        stateforward_hsm::behavior_context::current_operation_name().unwrap_or_default();
    inst.run_guard_behavior(ctx, event, &operation_name)
}

#[tokio::main]
async fn main() {
    let exit_code = match run_cli().await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            2
        }
    };
    std::process::exit(exit_code);
}

async fn run_cli() -> ConfResult<i32> {
    let roots: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    let roots = if roots.is_empty() {
        vec![PathBuf::from("../conformance/cases")]
    } else {
        roots
    };
    let files = collect_case_files(&roots)?;
    if files.is_empty() {
        return Err(ConfError::Fail(
            "no conformance case files found".to_string(),
        ));
    }

    let mut passed = 0;
    let mut skipped = 0;
    let mut failed = 0;

    for file in &files {
        match run_file(file).await {
            CaseResult::Pass { name } => {
                passed += 1;
                println!("PASS {name}");
            }
            CaseResult::Skip { name, reason } => {
                skipped += 1;
                println!("SKIP {name}: {reason}");
            }
            CaseResult::Fail { name, error } => {
                failed += 1;
                println!("FAIL {name}: {error}");
            }
        }
    }

    println!(
        "summary: pass={passed} skip={skipped} fail={failed} total={}",
        files.len()
    );

    if failed > 0 {
        Ok(1)
    } else if passed == 0 {
        Ok(77)
    } else {
        Ok(0)
    }
}

async fn run_file(file: &Path) -> CaseResult {
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) => {
            return CaseResult::Fail {
                name: file.display().to_string(),
                error: error.to_string(),
            };
        }
    };

    let case = match parse_case(&text) {
        Ok(case) => case,
        Err(error) => {
            return CaseResult::Fail {
                name: file.display().to_string(),
                error: error.to_string(),
            };
        }
    };

    let case_name = case.name.clone();
    match run_case(case).await {
        Ok(()) => CaseResult::Pass { name: case_name },
        Err(ConfError::Skip(reason)) => CaseResult::Skip {
            name: case_name,
            reason,
        },
        Err(ConfError::Fail(error)) => CaseResult::Fail {
            name: case_name,
            error,
        },
    }
}

async fn run_case(case: CaseData) -> ConfResult<()> {
    reset_case_state();

    let unsupported = unsupported_features(&case.features);
    if !unsupported.is_empty() {
        return Err(ConfError::Skip(format!(
            "unsupported features: {}",
            unsupported.join(", ")
        )));
    }

    match case.mode.as_str() {
        "runtime" => run_runtime_case(case).await,
        "validation" => run_validation_case(&case),
        other => Err(ConfError::Skip(format!("unsupported mode \"{other}\""))),
    }
}

async fn run_runtime_case(case: CaseData) -> ConfResult<()> {
    let ctx = Context::new();
    let clock = Arc::new(LogicalClock::default());
    set_current_clock(clock.clone());
    set_activity_trace_options(&case.expect);
    let models = build_model_registry(&case)?;
    let shared_groups = Arc::new(Mutex::new(BTreeMap::new()));
    let pending_group_dispatches = Arc::new(Mutex::new(VecDeque::new()));
    let defer_trace = Arc::new(Mutex::new(DeferTraceState {
        trace_defer: expected_trace_contains(&case.expect, "defer"),
        trace_undefer: expected_trace_contains(&case.expect, "undefer"),
        ..DeferTraceState::default()
    }));
    let hsms = build_instances(
        &ctx,
        &case,
        &models,
        shared_groups.clone(),
        pending_group_dispatches.clone(),
        defer_trace.clone(),
        clock.clone(),
    )?;
    let groups = build_groups(&case, &hsms)?;
    *shared_groups.lock().unwrap() = groups.clone();
    let trace_steps = StepTraceOptions {
        set: expected_trace_contains(&case.expect, "set"),
        start: expected_trace_contains(&case.expect, "start"),
        restart: expected_trace_contains(&case.expect, "restart"),
        stop: expected_trace_contains(&case.expect, "stop"),
        timer_cancelled: expected_trace_contains(&case.expect, "timer_cancelled"),
    };
    let mut last_instance_id = primary_instance_id(&hsms)?;
    let mut stable_label = None;
    let mut last_error: Option<(String, String)> = None;

    for step in &case.script {
        let step_object = object(step)?;
        let step_op = required_string(step_object, "op")?;
        let instance_id = if step_op == "dispatch_to"
            || step_op == "dispatch_all"
            || step_op == "group_dispatch"
            || step_op == "tick"
            || (step_op == "snapshot" && step_object.contains_key("group"))
        {
            primary_instance_id(&hsms)?
        } else {
            step_instance_id(step)?
        };
        let hsm = hsm_by_id(&hsms, &instance_id)?;
        match execute_step(&ctx, hsm, &groups, &clock, step, trace_steps).await {
            Ok(Some(label)) => stable_label = Some(label),
            Ok(None) if step_op != "tick" => stable_label = None,
            Ok(None) => {}
            Err(error) => {
                if record_expected_runtime_error(&case.expect, hsm, error, &mut last_error)? {
                    last_instance_id = instance_id.clone();
                    break;
                }
            }
        }
        if let Err(error) = drain_pending_group_dispatches(&pending_group_dispatches, &groups).await
        {
            if record_expected_runtime_error(&case.expect, hsm, error, &mut last_error)? {
                last_instance_id = instance_id.clone();
                break;
            }
        }
        flush_async_work().await;
        for hsm in hsms.values() {
            if let Err(error) = take_runtime_issue(hsm) {
                if record_expected_runtime_error(&case.expect, hsm, error, &mut last_error)? {
                    stable_label = None;
                    last_instance_id = hsm.id();
                    break;
                }
            }
        }
        if last_error.is_some() {
            break;
        }
        if step_op == "dispatch_to" && has_feature(&case, "model_registry") {
            if let Some(target_id) = stable_label
                .as_ref()
                .filter(|target| hsms.contains_key(*target))
            {
                last_instance_id = target_id.clone();
                stable_label = None;
            } else {
                last_instance_id = instance_id;
            }
        } else if step_op != "tick" {
            last_instance_id = instance_id;
        }
    }

    if let Some(label) = stable_label {
        append_stable_trace_label(hsm_by_id(&hsms, &last_instance_id)?, label);
    } else {
        append_stable_trace_for_expectation(hsm_by_id(&hsms, &last_instance_id)?, &case.expect);
    }

    assert_expectations(&case.expect, &hsms, last_error.as_ref())?;
    Ok(())
}

fn build_instances(
    ctx: &Context,
    case: &CaseData,
    models: &BTreeMap<String, Model<ConformanceInstance>>,
    groups: Arc<Mutex<BTreeMap<String, Group<ConformanceInstance>>>>,
    pending_group_dispatches: Arc<Mutex<VecDeque<PendingGroupDispatch>>>,
    defer_trace: Arc<Mutex<DeferTraceState>>,
    clock: Arc<LogicalClock>,
) -> ConfResult<BTreeMap<String, HSM<ConformanceInstance>>> {
    let mut hsms = BTreeMap::new();
    let trace_call_behaviors = trace_call_behavior_ids(case)?;
    let shared_trace = Arc::new(Mutex::new(Vec::new()));
    let root_name = model_name(&case.model)?;
    let root_model = models
        .get(&root_name)
        .cloned()
        .ok_or_else(|| ConfError::Fail(format!("missing model \"{root_name}\"")))?;
    if case.instances.is_empty() {
        let mut config = Config();
        config.ID = Some("default".to_string());
        let instance_data = ConformanceInstance::new_with_trace(
            case.behaviors.clone(),
            case.operation_aliases.clone(),
            case.combined_guards.clone(),
            trace_call_behaviors,
            groups,
            pending_group_dispatches,
            shared_trace,
            defer_trace.clone(),
        );
        config.Clock = Some(clock_fixture(
            None,
            clock.clone(),
            &instance_data,
            expected_trace_contains(&case.expect, "timer_scheduled"),
            expected_trace_contains(&case.expect, "timer_fired"),
        )?);
        let hsm = start_with_config(ctx, instance_data, root_model.clone(), config)?;
        defer_trace
            .lock()
            .unwrap()
            .models
            .insert("default".to_string(), root_model);
        hsms.insert("default".to_string(), hsm);
        return Ok(hsms);
    }

    let mut seen = BTreeSet::new();
    for instance_ir in &case.instances {
        let instance = object(instance_ir)?;
        let id = required_string(instance, "id")?;
        if !seen.insert(id.clone()) {
            validation_fail::<()>("duplicate_instance", format!("duplicate instance \"{id}\""))?;
        }
        let instance_data = ConformanceInstance::new_with_trace(
            case.behaviors.clone(),
            case.operation_aliases.clone(),
            case.combined_guards.clone(),
            trace_call_behaviors.clone(),
            groups.clone(),
            pending_group_dispatches.clone(),
            shared_trace.clone(),
            defer_trace.clone(),
        );
        let config = runtime_config_from_instance(
            instance,
            &id,
            &instance_data,
            clock.clone(),
            expected_trace_contains(&case.expect, "timer_scheduled"),
            expected_trace_contains(&case.expect, "timer_fired"),
        )?;
        let model_name = optional_string(instance, "model")?.unwrap_or_else(|| root_name.clone());
        let model = match models.get(&model_name).cloned() {
            Some(model) => model,
            None => {
                instance_data.error(
                    "model_error".to_string(),
                    format!("missing model \"{model_name}\""),
                );
                root_model.clone()
            }
        };
        let hsm = start_with_config(ctx, instance_data, model.clone(), config)?;
        defer_trace.lock().unwrap().models.insert(id.clone(), model);
        hsms.insert(id, hsm);
    }

    Ok(hsms)
}

fn build_groups(
    case: &CaseData,
    hsms: &BTreeMap<String, HSM<ConformanceInstance>>,
) -> ConfResult<BTreeMap<String, Group<ConformanceInstance>>> {
    validate_group_irs(case)?;
    let mut groups = BTreeMap::new();
    for group_ir in &case.groups {
        let group = object(group_ir)?;
        let id = required_string(group, "id")?;
        let members = group_members(group)?;
        let machines = members
            .iter()
            .map(|member| hsm_by_id(hsms, member).cloned())
            .collect::<ConfResult<Vec<_>>>()?;
        groups.insert(id.clone(), Group::with_id(id, machines));
    }
    Ok(groups)
}

fn trace_call_behavior_ids(case: &CaseData) -> ConfResult<BTreeSet<String>> {
    let mut behaviors = BTreeSet::new();
    if !expected_trace_contains(&case.expect, "call") {
        return Ok(behaviors);
    }
    collect_trace_call_behavior_ids(&case.model, &mut behaviors)?;
    for model in &case.models {
        collect_trace_call_behavior_ids(model, &mut behaviors)?;
    }
    Ok(behaviors)
}

fn collect_trace_call_behavior_ids(
    model: &Json,
    behaviors: &mut BTreeSet<String>,
) -> ConfResult<()> {
    for state in optional_array(object(model)?, "states")? {
        collect_state_trace_call_behavior_ids(state, behaviors)?;
    }
    Ok(())
}

fn collect_state_trace_call_behavior_ids(
    state: &Json,
    behaviors: &mut BTreeSet<String>,
) -> ConfResult<()> {
    let state = object(state)?;
    for key in ["entry", "exit", "activity"] {
        for ref_ir in optional_array(state, key)? {
            behaviors.insert(behavior_ref(ref_ir)?);
        }
    }
    for child in optional_array(state, "states")? {
        collect_state_trace_call_behavior_ids(child, behaviors)?;
    }
    Ok(())
}

fn runtime_config_from_instance(
    instance: &BTreeMap<String, Json>,
    id: &str,
    trace: &ConformanceInstance,
    clock: Arc<LogicalClock>,
    trace_timer_scheduled: bool,
    trace_timer_fired: bool,
) -> ConfResult<RuntimeConfig> {
    let mut config = Config();
    config.ID = Some(id.to_string());
    if let Some(data) = instance.get("data") {
        config.Data = Some(Arc::new(data.clone()));
    }

    if let Some(config_ir) = optional_object(instance, "config")? {
        if let Some(name) = optional_string(config_ir, "name")? {
            config.Name = Some(name);
        }
        if let Some(data) = config_ir.get("data") {
            config.Data = Some(Arc::new(data.clone()));
        }
        if let Some(queue_name) =
            optional_string(config_ir, "queue")?.or(optional_string(config_ir, "Queue")?)
        {
            config.Queue = Some(queue_fixture(&queue_name, trace, trace_timer_fired)?);
        }
        let clock_name =
            optional_string(config_ir, "clock")?.or(optional_string(config_ir, "Clock")?);
        config.Clock = Some(clock_fixture(
            clock_name.as_deref(),
            clock,
            trace,
            trace_timer_scheduled,
            trace_timer_fired && config.Queue.is_none(),
        )?);
    } else {
        config.Clock = Some(clock_fixture(
            None,
            clock,
            trace,
            trace_timer_scheduled,
            trace_timer_fired,
        )?);
    }

    Ok(config)
}

fn clock_fixture(
    name: Option<&str>,
    clock: Arc<LogicalClock>,
    trace: &ConformanceInstance,
    trace_timer_scheduled: bool,
    trace_timer_fired: bool,
) -> ConfResult<Clock> {
    if let Some(name) = name {
        if !matches!(
            name,
            "trace_no_sleep" | "trace_yield_sleep" | "trace_nonzero_sleep"
        ) {
            return Err(ConfError::Skip(format!(
                "unsupported clock fixture \"{name}\""
            )));
        }
    }
    let name = name.map(str::to_string);
    let trace = trace.clone();
    Ok(Clock {
        Sleep: Some(Arc::new(move |duration| {
            clock.sleep(
                duration,
                name.as_deref(),
                trace.clone(),
                trace_timer_scheduled,
                trace_timer_fired,
            )
        })),
    })
}

fn queue_fixture(
    name: &str,
    trace: &ConformanceInstance,
    trace_timer_fired: bool,
) -> ConfResult<RuntimeQueue> {
    let events = Arc::new(Mutex::new(VecDeque::<Event>::new()));
    let push_events = events.clone();
    let pop_events = events.clone();
    let len_events = events;
    let push_trace = trace.clone();
    let pop_trace = trace.clone();
    let len_trace = trace.clone();
    let lifo = name == "trace_lifo";
    let len_seven = name == "len_seven";
    let push_error = name == "push_error";
    let pop_error_pending = Arc::new(Mutex::new(name == "pop_error_once"));
    let pop_error = pop_error_pending.clone();
    let len_error_pending = Arc::new(Mutex::new(name == "len_error_once"));
    let len_error = len_error_pending.clone();

    match name {
        "trace_fifo" | "trace_lifo" | "len_seven" | "push_error" | "pop_error_once"
        | "len_error_once" => Ok(Queue(
            Arc::new(move |_ctx, event| {
                if push_error {
                    push_trace.push_trace(Json::object(vec![
                        ("type", Json::String("trace".to_string())),
                        (
                            "value",
                            Json::String(format!("queue:push-error:{}", event.name)),
                        ),
                    ]));
                    push_trace.error("runtime_error".to_string(), "queue push error".to_string());
                    return Err(HsmError::Runtime("queue push error".to_string()));
                }

                push_trace.push_trace(Json::object(vec![
                    ("type", Json::String("trace".to_string())),
                    ("value", Json::String(format!("queue:push:{}", event.name))),
                ]));
                push_events.lock().unwrap().push_back(event);
                Ok(())
            }),
            Arc::new(move |_ctx| {
                if pop_events.lock().unwrap().is_empty() {
                    return Ok(None);
                }
                let pop_should_error = {
                    let mut pop_error = pop_error.lock().unwrap();
                    let pop_should_error = *pop_error;
                    *pop_error = false;
                    pop_should_error
                };
                if pop_should_error {
                    pop_trace.push_trace(Json::object(vec![
                        ("type", Json::String("trace".to_string())),
                        ("value", Json::String("queue:pop-error".to_string())),
                    ]));
                    return Err(HsmError::Runtime("queue pop error".to_string()));
                }

                let event = if lifo {
                    pop_events.lock().unwrap().pop_back()
                } else {
                    pop_events.lock().unwrap().pop_front()
                };
                if let Some(event) = &event {
                    pop_trace.push_trace(Json::object(vec![
                        ("type", Json::String("trace".to_string())),
                        ("value", Json::String(format!("queue:pop:{}", event.name))),
                    ]));
                    if trace_timer_fired && is_registered_timer_event(&event.name) {
                        pop_trace.push_trace(Json::object(vec![(
                            "type",
                            Json::String("timer_fired".to_string()),
                        )]));
                    }
                }
                Ok(event)
            }),
            Arc::new(move |_ctx| {
                if len_seven {
                    return Ok(7);
                }
                let len_should_error = {
                    let mut len_error = len_error.lock().unwrap();
                    let len_should_error = *len_error;
                    *len_error = false;
                    len_should_error
                };
                if len_should_error {
                    len_trace.push_trace(Json::object(vec![
                        ("type", Json::String("trace".to_string())),
                        ("value", Json::String("queue:len-error".to_string())),
                    ]));
                    return Err(HsmError::Runtime("queue len error".to_string()));
                }
                Ok(len_events.lock().unwrap().len())
            }),
        )),
        other => Err(ConfError::Skip(format!(
            "unsupported queue fixture \"{other}\""
        ))),
    }
}

fn step_instance_id(step: &Json) -> ConfResult<String> {
    optional_string(object(step)?, "instance").map(|id| id.unwrap_or_else(|| "default".to_string()))
}

fn primary_instance_id(hsms: &BTreeMap<String, HSM<ConformanceInstance>>) -> ConfResult<String> {
    if hsms.contains_key("default") {
        return Ok("default".to_string());
    }
    hsms.keys()
        .next()
        .cloned()
        .ok_or_else(|| ConfError::Fail("no runtime instances built".to_string()))
}

fn hsm_by_id<'a>(
    hsms: &'a BTreeMap<String, HSM<ConformanceInstance>>,
    id: &str,
) -> ConfResult<&'a HSM<ConformanceInstance>> {
    hsms.get(id)
        .ok_or_else(|| ConfError::Fail(format!("unknown instance \"{id}\"")))
}

fn append_stable_trace_for_expectation(
    hsm: &HSM<ConformanceInstance>,
    expect: &BTreeMap<String, Json>,
) {
    let state = match expect.get("state") {
        Some(Json::String(_)) => hsm.state(),
        _ => expected_stable_state_from_trace(expect)
            .as_deref()
            .map(|_| hsm.state())
            .unwrap_or_else(|| hsm.state()),
    };
    append_stable_trace_label(hsm, state);
}

fn expected_stable_state_from_trace(expect: &BTreeMap<String, Json>) -> Option<String> {
    let Json::Array(trace) = expect.get("trace")? else {
        return None;
    };
    trace.iter().rev().find_map(|entry| {
        let entry = object(entry).ok()?;
        if optional_string(entry, "type").ok().flatten().as_deref() != Some("stable") {
            return None;
        }
        optional_string(entry, "state").ok().flatten()
    })
}

fn append_stable_trace_label(hsm: &HSM<ConformanceInstance>, state: String) {
    let instance = hsm.instance().read().unwrap();
    instance.push_trace(Json::object(vec![
        ("type", Json::String("stable".to_string())),
        ("state", Json::String(state)),
    ]));
}

fn record_expected_runtime_error(
    expect: &BTreeMap<String, Json>,
    hsm: &HSM<ConformanceInstance>,
    error: ConfError,
    last_error: &mut Option<(String, String)>,
) -> ConfResult<bool> {
    let ConfError::Fail(message) = error else {
        return Err(error);
    };
    let Some(expected_error) = optional_object(expect, "error")? else {
        return Err(ConfError::Fail(message));
    };
    let (code, message) = match message.split_once('\0') {
        Some((code, message)) => (code.to_string(), message.to_string()),
        None => (
            optional_string(expected_error, "code")?.unwrap_or_else(|| "runtime_error".to_string()),
            message,
        ),
    };
    append_error_trace(hsm, &code);
    *last_error = Some((code, message));
    Ok(true)
}

fn append_error_trace(hsm: &HSM<ConformanceInstance>, code: &str) {
    let instance = hsm.instance().read().unwrap();
    let mut trace = instance.trace.lock().unwrap();
    if trace.iter().any(|entry| {
        object(entry).ok().is_some_and(|entry| {
            optional_string(entry, "type").ok().flatten().as_deref() == Some("error")
                && optional_string(entry, "code").ok().flatten().as_deref() == Some(code)
        })
    }) {
        return;
    }
    trace.push(Json::object(vec![
        ("type", Json::String("error".to_string())),
        ("code", Json::String(code.to_string())),
    ]));
}

fn run_validation_case(case: &CaseData) -> ConfResult<()> {
    let Some(message) = validation_build_error(case)? else {
        return Err(ConfError::Fail(
            "validation case unexpectedly built successfully".to_string(),
        ));
    };

    if validation_error_matches(&case.expect, &message)? {
        Ok(())
    } else {
        Err(ConfError::Fail(format!(
            "validation error mismatch: {message:?}"
        )))
    }
}

fn validation_build_error(case: &CaseData) -> ConfResult<Option<String>> {
    match validate_behavior_programs(case) {
        Ok(()) => {}
        Err(ConfError::Fail(message)) => return Ok(Some(message)),
        Err(skip @ ConfError::Skip(_)) => return Err(skip),
    }

    match validate_instance_irs(case) {
        Ok(()) => {}
        Err(ConfError::Fail(message)) => return Ok(Some(message)),
        Err(skip @ ConfError::Skip(_)) => return Err(skip),
    }

    match validate_group_irs(case) {
        Ok(()) => {}
        Err(ConfError::Fail(message)) => return Ok(Some(message)),
        Err(skip @ ConfError::Skip(_)) => return Err(skip),
    }

    match validate_operation_declarations(case) {
        Ok(()) => {}
        Err(ConfError::Fail(message)) => return Ok(Some(message)),
        Err(skip @ ConfError::Skip(_)) => return Err(skip),
    }

    match validate_behavior_references(case) {
        Ok(()) => {}
        Err(ConfError::Fail(message)) => return Ok(Some(message)),
        Err(skip @ ConfError::Skip(_)) => return Err(skip),
    }

    match validate_model_registry(case) {
        Ok(()) => {}
        Err(ConfError::Fail(message)) => return Ok(Some(message)),
        Err(skip @ ConfError::Skip(_)) => return Err(skip),
    }

    match validate_model_ir_shape(
        object(&case.model)?,
        &case.operation_aliases,
        &case.behaviors,
    ) {
        Ok(()) => {}
        Err(ConfError::Fail(message)) => return Ok(Some(message)),
        Err(skip @ ConfError::Skip(_)) => return Err(skip),
    }

    for model in &case.models {
        match validate_model_ir_shape(object(model)?, &case.operation_aliases, &case.behaviors) {
            Ok(()) => {}
            Err(ConfError::Fail(message)) => return Ok(Some(message)),
            Err(skip @ ConfError::Skip(_)) => return Err(skip),
        }
    }

    let model = match build_model(case) {
        Ok(model) => model,
        Err(ConfError::Fail(message)) => return Ok(Some(message)),
        Err(skip @ ConfError::Skip(_)) => return Err(skip),
    };

    match validate(&model) {
        Ok(()) => Ok(None),
        Err(error) => Ok(Some(error.to_string())),
    }
}

fn validation_error_matches(expect: &BTreeMap<String, Json>, message: &str) -> ConfResult<bool> {
    let Some(Json::Array(expected)) = expect.get("validation") else {
        return Ok(true);
    };
    if expected.is_empty() {
        return Ok(true);
    }

    for item in expected {
        match item {
            Json::String(value) if message.contains(value) => return Ok(true),
            Json::Object(object) => {
                let contains_matches = optional_string(object, "message_contains")?
                    .as_ref()
                    .map_or(true, |contains| message.contains(contains));
                let code_matches = optional_string(object, "code")?
                    .as_ref()
                    .map_or(true, |code| validation_code_matches(code, message));
                if contains_matches && code_matches {
                    return Ok(true);
                }
            }
            _ => {}
        }
    }

    Ok(false)
}

fn validation_code_matches(code: &str, message: &str) -> bool {
    if message.contains(code) {
        return true;
    }
    if code == "missing_source" {
        return message.contains("source")
            && (message.contains("not found") || message.contains("required"));
    }

    let markers: &[&str] = match code {
        "invalid_name" => &["cannot contain", "name cannot be empty"],
        "invalid_attribute" => &["requires type or default", "default does not match"],
        "missing_initial" => &["Initial state is required", "requires initial"],
        "missing_target" => &[
            "not found",
            "target or effect is required",
            "must target inside",
        ],
        "invalid_final_transition" => &["Final state", "cannot have"],
        "missing_behavior" => &["missing operation"],
        "missing_operation" => &["missing operation"],
        "missing_submachine_model" => &["missing submachine model", "missing model"],
        "duplicate_model" => &["duplicate model"],
        "submachine_model_cycle" => &["submachine model cycle"],
        "invalid_submachine_initial" => &["submachine", "initial"],
        "invalid_submachine_contents" => &["submachine", "child states"],
        "invalid_submachine_internal_source" => &["internal source"],
        "invalid_submachine_internal_target" => &["internal state"],
        "invalid_submachine_boundary_target" => &["outside submachine boundary"],
        "missing_entry_point" => &["not found", "missing entry point"],
        "missing_exit_point" => &["not found", "missing exit point"],
        "invalid_entry_point_usage" => &["requires a submachine", "target or effects", "not found"],
        "invalid_exit_point_usage" => &["requires a submachine", "not found"],
        "invalid_entry_point_target" => &[
            "cannot target entry point",
            "cannot target exit point",
            "entry point target cannot be internal",
            "outside submachine boundary",
        ],
        "invalid_entry_point_target_kind" => {
            &["cannot target entry point", "cannot target exit point"]
        }
        "invalid_entry_point_internal_target" => &["entry point target cannot be internal"],
        "connection_point_name_collision" => &["connection_point_name_collision"],
        "choice_missing_fallback" => &["guardless fallback"],
        "choice_default_not_last" => &["fallback must be last"],
        "choice_missing_transition" => &["guardless fallback transition"],
        _ => &[],
    };

    markers.iter().any(|marker| message.contains(marker))
}

fn validation_fail<T>(code: &str, message: impl Into<String>) -> ConfResult<T> {
    Err(ConfError::Fail(format!("{code}: {}", message.into())))
}

fn validate_behavior_programs(case: &CaseData) -> ConfResult<()> {
    for (behavior_id, program) in &case.behaviors {
        if program.is_empty() {
            validation_fail::<()>(
                "empty_behavior_array",
                format!("behavior \"{behavior_id}\" must not be empty"),
            )?;
        }

        for op in program {
            let op_object = object(op).map_err(|_| {
                ConfError::Fail(format!(
                    "invalid_behavior_op_operand: behavior \"{behavior_id}\" op must be an object"
                ))
            })?;
            let op_name = required_string(op_object, "op").map_err(|_| {
                ConfError::Fail(format!(
                    "invalid_behavior_op_operand: behavior \"{behavior_id}\" op missing name"
                ))
            })?;
            validate_behavior_op(behavior_id, op_name.as_str(), op_object)?;
        }
    }

    Ok(())
}

fn validate_instance_irs(case: &CaseData) -> ConfResult<()> {
    let mut seen = BTreeSet::new();
    for instance_ir in &case.instances {
        let instance = object(instance_ir)?;
        let id = required_string(instance, "id")?;
        if !seen.insert(id.clone()) {
            validation_fail::<()>("duplicate_instance", format!("duplicate instance \"{id}\""))?;
        }
    }
    Ok(())
}

fn validate_group_irs(case: &CaseData) -> ConfResult<()> {
    let mut instance_ids = BTreeSet::new();
    if case.instances.is_empty() {
        instance_ids.insert("default".to_string());
    }
    for instance_ir in &case.instances {
        instance_ids.insert(required_string(object(instance_ir)?, "id")?);
    }

    let mut group_ids = BTreeSet::new();
    for group_ir in &case.groups {
        let group = object(group_ir)?;
        let id = required_string(group, "id")?;
        if !group_ids.insert(id.clone()) {
            validation_fail::<()>("duplicate_group", format!("duplicate group \"{id}\""))?;
        }
    }

    for group_ir in &case.groups {
        let group = object(group_ir)?;
        let id = required_string(group, "id")?;
        let members = group_members(group)?;
        if members.len() < 2 {
            validation_fail::<()>(
                "invalid_group_cardinality",
                format!("group \"{id}\" must contain at least two members"),
            )?;
        }
        let mut seen_members = BTreeSet::new();
        for member in members {
            if !seen_members.insert(member.clone()) {
                validation_fail::<()>(
                    "duplicate_group_member",
                    format!("duplicate group member \"{member}\""),
                )?;
            }
            if !instance_ids.contains(&member) {
                validation_fail::<()>(
                    "unknown_group_member",
                    format!("unknown group member \"{member}\""),
                )?;
            }
        }
    }

    Ok(())
}

fn group_members(group: &BTreeMap<String, Json>) -> ConfResult<Vec<String>> {
    let Some(members) = group.get("members") else {
        return Ok(Vec::new());
    };
    let Json::Array(members) = members else {
        return Err(ConfError::Fail(
            "group.members must be an array".to_string(),
        ));
    };
    members.iter().map(string).collect()
}

fn validate_operation_declarations(case: &CaseData) -> ConfResult<()> {
    for (operation, behavior_id) in &case.operation_aliases {
        if operation.contains('/') {
            validation_fail::<()>(
                "invalid_name",
                format!("operation name \"{operation}\" cannot contain \"/\""),
            )?;
        }
        if !case.behaviors.contains_key(behavior_id) {
            validation_fail::<()>(
                "missing_behavior",
                format!("operation \"{operation}\" missing behavior \"{behavior_id}\""),
            )?;
        }
    }
    Ok(())
}

fn validate_behavior_references(case: &CaseData) -> ConfResult<()> {
    validate_model_behavior_references(&case.model, &case.behaviors)?;
    for model in &case.models {
        validate_model_behavior_references(model, &case.behaviors)?;
    }
    Ok(())
}

fn validate_model_behavior_references(
    model: &Json,
    behaviors: &HashMap<String, Vec<Json>>,
) -> ConfResult<()> {
    let model = object(model)?;
    for point in optional_array(model, "entry_points")? {
        validate_connection_point_behavior_references(point, behaviors)?;
    }
    for point in optional_array(model, "exit_points")? {
        validate_connection_point_behavior_references(point, behaviors)?;
    }
    for transition in optional_array(model, "transitions")? {
        validate_transition_behavior_references(transition, behaviors)?;
    }
    if let Some(initial) = model.get("initial") {
        validate_initial_behavior_references(initial, behaviors)?;
    }
    for state in optional_array(model, "states")? {
        validate_state_behavior_references(state, behaviors)?;
    }
    Ok(())
}

fn validate_connection_point_behavior_references(
    point: &Json,
    behaviors: &HashMap<String, Vec<Json>>,
) -> ConfResult<()> {
    for ref_ir in optional_array(object(point)?, "effects")? {
        validate_behavior_ref_exists(ref_ir, behaviors)?;
    }
    Ok(())
}

fn validate_state_behavior_references(
    state: &Json,
    behaviors: &HashMap<String, Vec<Json>>,
) -> ConfResult<()> {
    let state = object(state)?;
    for key in ["entry", "exit", "activity"] {
        for ref_ir in optional_array(state, key)? {
            validate_behavior_ref_exists(ref_ir, behaviors)?;
        }
    }
    if let Some(initial) = state.get("initial") {
        validate_initial_behavior_references(initial, behaviors)?;
    }
    for transition in optional_array(state, "transitions")? {
        validate_transition_behavior_references(transition, behaviors)?;
    }
    for child in optional_array(state, "states")? {
        validate_state_behavior_references(child, behaviors)?;
    }
    Ok(())
}

fn validate_initial_behavior_references(
    initial: &Json,
    behaviors: &HashMap<String, Vec<Json>>,
) -> ConfResult<()> {
    if let Json::Object(object) = initial {
        for ref_ir in optional_array(object, "effects")? {
            validate_behavior_ref_exists(ref_ir, behaviors)?;
        }
    }
    Ok(())
}

fn validate_transition_behavior_references(
    transition: &Json,
    behaviors: &HashMap<String, Vec<Json>>,
) -> ConfResult<()> {
    let transition = object(transition)?;
    if let Some(guard) = transition.get("guard") {
        validate_behavior_ref_exists(guard, behaviors)?;
    }
    for ref_ir in optional_array(transition, "effects")? {
        validate_behavior_ref_exists(ref_ir, behaviors)?;
    }
    Ok(())
}

fn validate_behavior_ref_exists(
    ref_ir: &Json,
    behaviors: &HashMap<String, Vec<Json>>,
) -> ConfResult<()> {
    let behavior = behavior_ref(ref_ir)?;
    if !behaviors.contains_key(&behavior) {
        validation_fail::<()>(
            "missing_behavior",
            format!("missing behavior \"{behavior}\""),
        )?;
    }
    Ok(())
}

fn validate_model_registry(case: &CaseData) -> ConfResult<()> {
    let root_name = model_name(&case.model)?;
    let models = child_model_map(case)?;
    validate_submachine_model_references(&case.model, &models)?;
    for model in &case.models {
        validate_submachine_model_references(model, &models)?;
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for name in models.keys() {
        validate_submachine_model_cycle(name, &models, &mut visiting, &mut visited)?;
    }

    if models.contains_key(&root_name) {
        validation_fail::<()>(
            "duplicate_model",
            format!("child model \"{root_name}\" duplicates root model"),
        )?;
    }

    Ok(())
}

fn child_model_map(case: &CaseData) -> ConfResult<BTreeMap<String, Json>> {
    let root_name = model_name(&case.model)?;
    let mut models = BTreeMap::new();
    for model in &case.models {
        let name = model_name(model)?;
        if name.contains('/') {
            validation_fail::<()>(
                "invalid_name",
                format!("child model name \"{name}\" cannot contain \"/\""),
            )?;
        }
        if name == root_name || models.insert(name.clone(), model.clone()).is_some() {
            validation_fail::<()>(
                "duplicate_model",
                format!("duplicate child model \"{name}\""),
            )?;
        }
    }
    Ok(models)
}

fn validate_submachine_model_references(
    model: &Json,
    models: &BTreeMap<String, Json>,
) -> ConfResult<()> {
    for machine in submachine_references(model)? {
        if machine.contains('/') {
            validation_fail::<()>(
                "invalid_name",
                format!("child model name \"{machine}\" cannot contain \"/\""),
            )?;
        }
        if !models.contains_key(&machine) {
            validation_fail::<()>(
                "missing_submachine_model",
                format!("missing submachine model \"{machine}\""),
            )?;
        }
    }
    Ok(())
}

fn validate_submachine_model_cycle(
    name: &str,
    models: &BTreeMap<String, Json>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> ConfResult<()> {
    if visited.contains(name) {
        return Ok(());
    }
    if !visiting.insert(name.to_string()) {
        validation_fail::<()>(
            "submachine_model_cycle",
            format!("submachine model cycle includes \"{name}\""),
        )?;
    }
    if let Some(model) = models.get(name) {
        for child in submachine_references(model)? {
            if models.contains_key(&child) {
                validate_submachine_model_cycle(&child, models, visiting, visited)?;
            }
        }
    }
    visiting.remove(name);
    visited.insert(name.to_string());
    Ok(())
}

fn submachine_references(model: &Json) -> ConfResult<Vec<String>> {
    let mut references = Vec::new();
    for state in optional_array(object(model)?, "states")? {
        collect_state_submachine_references(state, &mut references)?;
    }
    Ok(references)
}

fn collect_state_submachine_references(
    state: &Json,
    references: &mut Vec<String>,
) -> ConfResult<()> {
    let state = object(state)?;
    if optional_string(state, "kind")?.as_deref() == Some("submachine") {
        references.push(required_string(state, "machine")?);
    }
    for child in optional_array(state, "states")? {
        collect_state_submachine_references(child, references)?;
    }
    Ok(())
}

fn validate_behavior_op(
    behavior_id: &str,
    op_name: &str,
    op: &BTreeMap<String, Json>,
) -> ConfResult<()> {
    let invalid = |message: &str| {
        validation_fail::<()>(
            "invalid_behavior_op_operand",
            format!("behavior \"{behavior_id}\" {op_name}: {message}"),
        )
    };

    match op_name {
        "call" => {
            require_behavior_operand(op, "name", invalid)?;
            reject_behavior_operand(op, "event", invalid)
        }
        "dispatch" => {
            require_behavior_operand(op, "event", invalid)?;
            reject_behavior_operand(op, "name", invalid)?;
            if op.contains_key("target") && op.contains_key("group") {
                invalid("target and group are mutually exclusive")?;
            }
            Ok(())
        }
        "event_data_equals" => {
            require_behavior_operand(op, "path", invalid)?;
            require_behavior_operand(op, "value", invalid)
        }
        "event_data_get" => {
            require_behavior_operand(op, "path", invalid)?;
            reject_behavior_operand(op, "value", invalid)
        }
        "event_metadata_equals" => {
            require_behavior_operand(op, "name", invalid)?;
            require_behavior_operand(op, "value", invalid)
        }
        "event_application_metadata_equals" => {
            require_behavior_operand(op, "name", invalid)?;
            require_behavior_operand(op, "value", invalid)
        }
        "event_metadata_get" => require_behavior_operand(op, "name", invalid),
        "event_metadata_set" => {
            require_behavior_operand(op, "name", invalid)?;
            require_behavior_operand(op, "value", invalid)
        }
        "event_name_equals" => require_behavior_operand(op, "value", invalid),
        "get_attr" => require_behavior_operand(op, "name", invalid),
        "raise" => {
            let has_event = op.contains_key("event");
            let has_code = op.contains_key("code");
            if has_event == has_code {
                invalid("raise requires exactly one of event or code")?;
            }
            Ok(())
        }
        "return_attr" => require_behavior_operand(op, "name", invalid),
        "return_equals" => {
            require_behavior_operand(op, "name", invalid)?;
            require_behavior_operand(op, "value", invalid)
        }
        "return_value" => require_behavior_operand(op, "value", invalid),
        "set_attr" => {
            require_behavior_operand(op, "name", invalid)?;
            require_behavior_operand(op, "value", invalid)?;
            reject_behavior_operand(op, "event", invalid)
        }
        "set_attr_from_event_data" => {
            require_behavior_operand(op, "name", invalid)?;
            require_behavior_operand(op, "path", invalid)
        }
        "sleep" => {
            require_behavior_operand(op, "millis", invalid)?;
            reject_behavior_operand(op, "event", invalid)
        }
        "snapshot" => reject_behavior_operand(op, "event", invalid),
        "trace" => require_behavior_operand(op, "value", invalid),
        "yield" => reject_behavior_operand(op, "value", invalid),
        _ => Ok(()),
    }
}

fn require_behavior_operand(
    op: &BTreeMap<String, Json>,
    key: &str,
    invalid: impl Fn(&str) -> ConfResult<()>,
) -> ConfResult<()> {
    if op.contains_key(key) {
        Ok(())
    } else {
        invalid(&format!("missing operand \"{key}\""))
    }
}

fn reject_behavior_operand(
    op: &BTreeMap<String, Json>,
    key: &str,
    invalid: impl Fn(&str) -> ConfResult<()>,
) -> ConfResult<()> {
    if op.contains_key(key) {
        invalid(&format!("extraneous operand \"{key}\""))
    } else {
        Ok(())
    }
}

fn validate_model_ir_shape(
    model: &BTreeMap<String, Json>,
    operations: &HashMap<String, String>,
    behaviors: &HashMap<String, Vec<Json>>,
) -> ConfResult<()> {
    let mut attribute_types = AttributeTypeMap::new();
    collect_attribute_types(model, &mut attribute_types)?;
    let entry_points = optional_array(model, "entry_points")?;
    let exit_points = optional_array(model, "exit_points")?;
    let states = optional_array(model, "states")?;
    validate_entry_point_shapes(entry_points.clone())?;
    validate_exit_point_shapes(exit_points.clone())?;
    validate_connection_point_name_collisions(&states, &entry_points, &exit_points)?;
    validate_initial_shape(model.get("initial"))?;
    for transition in optional_array(model, "transitions")? {
        validate_transition_shape(object(transition)?, operations, behaviors, &attribute_types)?;
    }
    let model_name = required_string(model, "name")?;
    let model_root = format!("/{model_name}");
    validate_state_list(
        states,
        operations,
        behaviors,
        &attribute_types,
        &model_root,
        &model_root,
    )
}

fn collect_attribute_types(
    owner: &BTreeMap<String, Json>,
    attributes: &mut AttributeTypeMap,
) -> ConfResult<()> {
    if let Some(declared) = optional_object(owner, "attributes")? {
        for (name, spec) in declared {
            let attribute_type = match spec {
                Json::Object(object) => optional_string(object, "type")?
                    .or_else(|| attribute_type_name_from_default(object.get("default"))),
                value => attribute_type_name_from_default(Some(value)),
            };
            if let Some(attribute_type) = attribute_type {
                attributes.insert(name.clone(), attribute_type);
            }
        }
    }

    for state in optional_array(owner, "states")? {
        collect_attribute_types(object(state)?, attributes)?;
    }
    Ok(())
}

fn attribute_type_name_from_default(value: Option<&Json>) -> Option<String> {
    match value {
        Some(Json::Bool(_)) => Some("boolean".to_string()),
        Some(Json::Number(_)) => Some("duration_ms".to_string()),
        Some(Json::String(_)) => Some("string".to_string()),
        Some(Json::Array(_)) => Some("array".to_string()),
        Some(Json::Object(_)) => Some("object".to_string()),
        Some(Json::Null) => Some("null".to_string()),
        None => None,
    }
}

fn validate_connection_point_name_collisions(
    states: &[&Json],
    entry_points: &[&Json],
    exit_points: &[&Json],
) -> ConfResult<()> {
    let mut state_names = BTreeSet::new();
    for state in states {
        state_names.insert(required_string(object(state)?, "name")?);
    }
    for point in entry_points.iter().chain(exit_points.iter()) {
        let name = required_string(object(point)?, "name")?;
        if state_names.contains(&name) {
            validation_fail::<()>(
                "connection_point_name_collision",
                format!("connection point \"{name}\" collides with state name"),
            )?;
        }
    }
    Ok(())
}

fn validate_entry_point_shapes(points: Vec<&Json>) -> ConfResult<()> {
    let mut names = BTreeSet::new();
    for point in points {
        let point = object(point)?;
        let name = required_string(point, "name")?;
        if name.contains('/') {
            validation_fail::<()>(
                "invalid_name",
                format!("entry point name \"{name}\" cannot contain \"/\""),
            )?;
        }
        if !names.insert(name.clone()) {
            validation_fail::<()>(
                "duplicate_entry_point",
                format!("duplicate entry point \"{name}\""),
            )?;
        }
        required_string(point, "target")?;
        validate_non_empty_array(point, "effects", "empty_behavior_array")?;
    }
    Ok(())
}

fn validate_exit_point_shapes(points: Vec<&Json>) -> ConfResult<()> {
    let mut names = BTreeSet::new();
    for point in points {
        let point = object(point)?;
        let name = required_string(point, "name")?;
        if name.contains('/') {
            validation_fail::<()>(
                "invalid_name",
                format!("exit point name \"{name}\" cannot contain \"/\""),
            )?;
        }
        if !names.insert(name.clone()) {
            validation_fail::<()>(
                "duplicate_exit_point",
                format!("duplicate exit point \"{name}\""),
            )?;
        }
        validate_non_empty_array(point, "effects", "empty_behavior_array")?;
    }
    Ok(())
}

fn validate_state_list(
    states: Vec<&Json>,
    operations: &HashMap<String, String>,
    behaviors: &HashMap<String, Vec<Json>>,
    attribute_types: &AttributeTypeMap,
    model_root: &str,
    owner_path: &str,
) -> ConfResult<()> {
    let mut names = BTreeSet::new();
    for state in states {
        let state_object = object(state)?;
        let name = required_string(state_object, "name")?;
        if !names.insert(name.clone()) {
            validation_fail::<()>("duplicate_state", format!("duplicate state \"{name}\""))?;
        }
        validate_state_shape(
            state_object,
            operations,
            behaviors,
            attribute_types,
            model_root,
            owner_path,
        )?;
    }
    Ok(())
}

fn validate_state_shape(
    state: &BTreeMap<String, Json>,
    operations: &HashMap<String, String>,
    behaviors: &HashMap<String, Vec<Json>>,
    attribute_types: &AttributeTypeMap,
    model_root: &str,
    owner_path: &str,
) -> ConfResult<()> {
    let name = required_string(state, "name")?;
    if name.contains('/') {
        validation_fail::<()>(
            "invalid_name",
            format!("state name \"{name}\" cannot contain \"/\""),
        )?;
    }
    let kind = optional_string(state, "kind")?;
    if matches!(
        kind.as_deref(),
        Some("choice" | "shallow_history" | "deep_history")
    ) {
        if state.contains_key("initial") {
            validation_fail::<()>(
                "already has an initial state",
                format!("{kind:?} already has an initial state"),
            )?;
        }
        if matches!(kind.as_deref(), Some("shallow_history" | "deep_history")) {
            if owner_path == model_root {
                validation_fail::<()>(
                    "invalid_history_owner",
                    "history pseudostate must be within a nested state",
                )?;
            }
            if optional_array(state, "transitions")?.is_empty() {
                validation_fail::<()>(
                    "history_missing_default",
                    "history requires a default transition",
                )?;
            }
        }
        if state.contains_key("entry")
            || state.contains_key("exit")
            || state.contains_key("activity")
            || state.contains_key("defer")
            || state.contains_key("states")
        {
            validation_fail::<()>(
                "invalid_pseudostate_contents",
                "pseudostate cannot declare state-only contents",
            )?;
        }
    }

    if kind.as_deref() == Some("final")
        && (state.contains_key("transitions")
            || state.contains_key("entry")
            || state.contains_key("exit")
            || state.contains_key("activity")
            || state.contains_key("initial")
            || state.contains_key("defer")
            || state.contains_key("states"))
    {
        validation_fail::<()>(
            "invalid_final_transition",
            "final state cannot declare behavior, transitions, or child states",
        )?;
    }

    if kind.as_deref() == Some("submachine") {
        let machine = required_string(state, "machine")?;
        if machine.contains('/') {
            validation_fail::<()>(
                "invalid_name",
                format!("child model name \"{machine}\" cannot contain \"/\""),
            )?;
        }
        if state.contains_key("initial") {
            validation_fail::<()>(
                "invalid_submachine_initial",
                "submachine state cannot declare an initial target",
            )?;
        }
        if state.contains_key("states") {
            validation_fail::<()>(
                "invalid_submachine_contents",
                "submachine state cannot declare direct child states",
            )?;
        }
    }

    validate_non_empty_array(state, "entry", "empty_behavior_array")?;
    validate_non_empty_array(state, "exit", "empty_behavior_array")?;
    validate_non_empty_array(state, "activity", "empty_behavior_array")?;
    validate_non_empty_array(state, "defer", "empty_event_array")?;
    validate_initial_shape(state.get("initial"))?;
    for transition in optional_array(state, "transitions")? {
        validate_transition_shape(object(transition)?, operations, behaviors, attribute_types)?;
    }
    if kind.as_deref() == Some("choice") {
        validate_choice_shape(optional_array(state, "transitions")?)?;
    }
    if matches!(
        kind.as_deref(),
        Some("choice" | "submachine" | "shallow_history" | "deep_history")
    ) {
        return Ok(());
    }
    validate_state_list(
        optional_array(state, "states")?,
        operations,
        behaviors,
        attribute_types,
        model_root,
        &join_path(owner_path, &name),
    )
}

fn validate_choice_shape(transitions: Vec<&Json>) -> ConfResult<()> {
    let Some(last_transition) = transitions.last() else {
        validation_fail::<()>(
            "choice_missing_transition",
            "choice must declare transitions",
        )?;
        return Ok(());
    };
    for transition in transitions.iter().take(transitions.len().saturating_sub(1)) {
        if !object(transition)?.contains_key("guard") {
            validation_fail::<()>(
                "choice_default_not_last",
                "choice guardless fallback must be last",
            )?;
        }
    }
    let last_transition = object(last_transition)?;
    if last_transition.contains_key("guard") {
        validation_fail::<()>(
            "choice_missing_fallback",
            "choice requires a guardless fallback transition",
        )?;
    }
    Ok(())
}

fn validate_initial_shape(initial: Option<&Json>) -> ConfResult<()> {
    let Some(Json::Object(initial)) = initial else {
        return Ok(());
    };
    validate_non_empty_array(initial, "effects", "empty_behavior_array")
}

fn validate_transition_shape(
    transition: &BTreeMap<String, Json>,
    operations: &HashMap<String, String>,
    behaviors: &HashMap<String, Vec<Json>>,
    attribute_types: &AttributeTypeMap,
) -> ConfResult<()> {
    validate_non_empty_array(transition, "effects", "empty_behavior_array")?;
    if !transition.contains_key("target") && !transition.contains_key("effects") {
        validation_fail::<()>("missing_target", "transition requires target or effects")?;
    }
    if transition.contains_key("on") && transition.contains_key("trigger") {
        validation_fail::<()>(
            "multiple_transition_triggers",
            "transition cannot declare both on and trigger",
        )?;
    }
    if let Some(entry_point) = optional_string(transition, "entry_point")? {
        if entry_point.contains('/') {
            validation_fail::<()>(
                "invalid_name",
                format!("entry point selector \"{entry_point}\" cannot contain \"/\""),
            )?;
        }
    }
    if let Some(trigger) = optional_object(transition, "trigger")? {
        validate_trigger_shape(trigger, operations, behaviors, attribute_types)?;
    }
    Ok(())
}

fn validate_trigger_shape(
    trigger: &BTreeMap<String, Json>,
    operations: &HashMap<String, String>,
    behaviors: &HashMap<String, Vec<Json>>,
    attribute_types: &AttributeTypeMap,
) -> ConfResult<()> {
    let kind = required_string(trigger, "kind")?;
    match kind.as_str() {
        "on" => {
            let has_event = trigger.contains_key("event");
            let has_events = trigger.contains_key("events");
            if !has_event && !has_events {
                validation_fail::<()>("missing_trigger_operand", "on trigger missing event")?;
            }
            if has_event && has_events {
                validation_fail::<()>(
                    "multiple_trigger_operands",
                    "on trigger cannot declare event and events",
                )?;
            }
            validate_non_empty_array(trigger, "events", "empty_event_array")?;
            reject_trigger_operands(
                trigger,
                &[
                    "attribute",
                    "behavior",
                    "operation",
                    "exit_point",
                    "duration_ms",
                    "time_ms",
                    "timer_source",
                ],
            )?;
            Ok(())
        }
        "on_set" => {
            if !trigger.contains_key("attribute") {
                validation_fail::<()>(
                    "missing_trigger_operand",
                    "on_set trigger missing attribute",
                )?;
            }
            reject_trigger_operands(
                trigger,
                &[
                    "event",
                    "events",
                    "duration_ms",
                    "time_ms",
                    "timer_source",
                    "exit_point",
                ],
            )?;
            Ok(())
        }
        "on_call" => {
            let operation = optional_string(trigger, "operation")?;
            let Some(operation) = operation else {
                validation_fail::<()>(
                    "missing_trigger_operand",
                    "on_call trigger missing operation",
                )?;
                return Ok(());
            };
            if operation.contains('/') {
                validation_fail::<()>(
                    "invalid_name",
                    format!("operation name \"{operation}\" cannot contain \"/\""),
                )?;
            }
            if !operations.contains_key(&operation) {
                validation_fail::<()>(
                    "missing_operation",
                    format!("on_call trigger missing operation \"{operation}\""),
                )?;
            }
            reject_trigger_operands(
                trigger,
                &[
                    "attribute",
                    "behavior",
                    "event",
                    "events",
                    "duration_ms",
                    "time_ms",
                    "timer_source",
                    "exit_point",
                ],
            )?;
            Ok(())
        }
        "exit_point" => {
            let exit_point = optional_string(trigger, "exit_point")?;
            let Some(exit_point) = exit_point else {
                validation_fail::<()>(
                    "missing_trigger_operand",
                    "exit_point trigger missing exit_point",
                )?;
                return Ok(());
            };
            if exit_point.contains('/') {
                validation_fail::<()>(
                    "invalid_name",
                    format!("exit point trigger \"{exit_point}\" cannot contain \"/\""),
                )?;
            }
            reject_trigger_operands(
                trigger,
                &[
                    "attribute",
                    "behavior",
                    "event",
                    "events",
                    "operation",
                    "duration_ms",
                    "time_ms",
                    "timer_source",
                ],
            )?;
            Ok(())
        }
        "completion" => {
            reject_trigger_operands(
                trigger,
                &[
                    "attribute",
                    "behavior",
                    "event",
                    "events",
                    "operation",
                    "duration_ms",
                    "time_ms",
                    "timer_source",
                    "exit_point",
                ],
            )?;
            Ok(())
        }
        "when" => {
            let has_attribute = trigger.contains_key("attribute");
            let behavior = optional_string(trigger, "behavior")?;
            if !has_attribute && behavior.is_none() {
                validation_fail::<()>(
                    "missing_trigger_operand",
                    "when trigger missing attribute or behavior",
                )?;
            }
            if has_attribute && behavior.is_some() {
                validation_fail::<()>(
                    "multiple_trigger_operands",
                    "when trigger cannot declare both attribute and behavior",
                )?;
            }
            if let Some(attribute) = optional_string(trigger, "attribute")? {
                if attribute.contains('/') {
                    validation_fail::<()>(
                        "invalid_name",
                        format!("attribute name \"{attribute}\" cannot contain \"/\""),
                    )?;
                }
            }
            if let Some(behavior) = behavior {
                if !behaviors.contains_key(&behavior) {
                    validation_fail::<()>(
                        "missing_behavior",
                        format!("when trigger missing behavior \"{behavior}\""),
                    )?;
                }
            }
            reject_trigger_operands(
                trigger,
                &[
                    "event",
                    "events",
                    "operation",
                    "duration_ms",
                    "time_ms",
                    "timer_source",
                ],
            )?;
            Ok(())
        }
        "after" => validate_timer_trigger_shape(
            trigger,
            "after",
            "duration_ms",
            "time_ms",
            "duration_ms",
            attribute_types,
            behaviors,
        ),
        "every" => validate_timer_trigger_shape(
            trigger,
            "every",
            "duration_ms",
            "time_ms",
            "duration_ms",
            attribute_types,
            behaviors,
        ),
        "at" => validate_timer_trigger_shape(
            trigger,
            "at",
            "time_ms",
            "duration_ms",
            "time_ms",
            attribute_types,
            behaviors,
        ),
        _ => Ok(()),
    }
}

fn validate_timer_trigger_shape(
    trigger: &BTreeMap<String, Json>,
    kind: &str,
    literal_field: &str,
    wrong_literal_field: &str,
    expected_attribute_type: &str,
    attribute_types: &AttributeTypeMap,
    behaviors: &HashMap<String, Vec<Json>>,
) -> ConfResult<()> {
    reject_trigger_operands(
        trigger,
        &["event", "events", "operation", "exit_point", "timer_source"],
    )?;

    let literal = trigger.contains_key(literal_field);
    let wrong_literal = trigger.contains_key(wrong_literal_field);
    let attribute = optional_string(trigger, "attribute")?;
    let behavior = optional_string(trigger, "behavior")?;
    let source_count =
        usize::from(literal) + usize::from(attribute.is_some()) + usize::from(behavior.is_some());

    if wrong_literal || source_count != 1 {
        validation_fail::<()>(
            "invalid_timer_source",
            format!("{kind} timer requires exactly one compatible source"),
        )?;
    }

    if kind == "every"
        && literal
        && json_u64(required_json(trigger, literal_field)?, literal_field)? == 0
    {
        validation_fail::<()>(
            "invalid_timer_source",
            "every timer interval must be nonzero",
        )?;
    }

    if let Some(attribute) = attribute {
        let Some(attribute_type) = attribute_types.get(&attribute) else {
            validation_fail::<()>(
                "missing_timer_attribute",
                format!("timer source attribute \"{attribute}\" is not declared"),
            )?;
            return Ok(());
        };
        if attribute_type != expected_attribute_type {
            validation_fail::<()>(
                "invalid_timer_attribute_type",
                format!(
                    "{kind} timer source attribute \"{attribute}\" must be {expected_attribute_type}"
                ),
            )?;
        }
    }

    if let Some(behavior) = behavior {
        let Some(program) = behaviors.get(&behavior) else {
            validation_fail::<()>(
                "missing_behavior",
                format!("{kind} timer missing behavior \"{behavior}\""),
            )?;
            return Ok(());
        };
        validate_timer_behavior_return(&behavior, program)?;
    }

    Ok(())
}

fn required_json<'a>(object: &'a BTreeMap<String, Json>, key: &str) -> ConfResult<&'a Json> {
    object
        .get(key)
        .ok_or_else(|| ConfError::Fail(format!("missing field \"{key}\"")))
}

fn validate_timer_behavior_return(behavior: &str, program: &[Json]) -> ConfResult<()> {
    for op in program {
        let op = object(op)?;
        if required_string(op, "op")?.as_str() != "return_value" {
            continue;
        }
        if matches!(op.get("value"), Some(Json::Number(_))) {
            return Ok(());
        }
        validation_fail::<()>(
            "invalid_timer_behavior_return",
            format!("timer behavior \"{behavior}\" must return a number"),
        )?;
    }
    Ok(())
}

fn reject_trigger_operands(trigger: &BTreeMap<String, Json>, operands: &[&str]) -> ConfResult<()> {
    for operand in operands {
        if trigger.contains_key(*operand) {
            validation_fail::<()>(
                "extraneous_trigger_operand",
                format!("trigger has extraneous operand \"{operand}\""),
            )?;
        }
    }
    Ok(())
}

fn validate_non_empty_array(
    object: &BTreeMap<String, Json>,
    key: &str,
    code: &str,
) -> ConfResult<()> {
    let Some(value) = object.get(key) else {
        return Ok(());
    };
    let Json::Array(values) = value else {
        return Err(ConfError::Fail(format!("field \"{key}\" must be an array")));
    };
    if values.is_empty() {
        validation_fail::<()>(code, format!("array field \"{key}\" must not be empty"))?;
    }
    Ok(())
}

fn build_model(case: &CaseData) -> ConfResult<Model<ConformanceInstance>> {
    let models = child_model_map(case)?;
    let mut build_stack = BTreeSet::new();
    build_model_ir(case, &case.model, &models, &mut build_stack)
}

fn build_model_registry(
    case: &CaseData,
) -> ConfResult<BTreeMap<String, Model<ConformanceInstance>>> {
    let model_irs = child_model_map(case)?;
    let mut registry = BTreeMap::new();
    let root_name = model_name(&case.model)?;
    let mut build_stack = BTreeSet::new();
    registry.insert(
        root_name,
        build_model_ir(case, &case.model, &model_irs, &mut build_stack)?,
    );

    for (name, model_ir) in &model_irs {
        let mut build_stack = BTreeSet::new();
        match build_model_ir(case, model_ir, &model_irs, &mut build_stack) {
            Ok(model) => {
                registry.insert(name.clone(), model);
            }
            Err(ConfError::Skip(_)) => {}
            Err(error) => return Err(error),
        }
    }

    Ok(registry)
}

fn build_model_ir(
    case: &CaseData,
    model_ir: &Json,
    models: &BTreeMap<String, Json>,
    build_stack: &mut BTreeSet<String>,
) -> ConfResult<Model<ConformanceInstance>> {
    build_model_ir_with_attributes(
        case,
        model_ir,
        models,
        build_stack,
        &AttributeEventMap::new(),
    )
}

fn build_model_ir_with_attributes(
    case: &CaseData,
    model_ir: &Json,
    models: &BTreeMap<String, Json>,
    build_stack: &mut BTreeSet<String>,
    inherited_attribute_events: &AttributeEventMap,
) -> ConfResult<Model<ConformanceInstance>> {
    let model_object = object(model_ir)?;
    let model_name = required_string(model_object, "name")?;
    if !build_stack.insert(model_name.clone()) {
        return Err(ConfError::Skip(format!(
            "submachine model cycle includes \"{model_name}\""
        )));
    }
    let base_model = if let Some(base_name) = optional_string(model_object, "redefines")? {
        let Some(base_ir) = models.get(&base_name) else {
            return Err(ConfError::Fail(format!(
                "missing_submachine_model: missing redefined model \"{base_name}\""
            )));
        };
        Some(build_model_ir_with_attributes(
            case,
            base_ir,
            models,
            build_stack,
            inherited_attribute_events,
        )?)
    } else {
        None
    };
    let model_root = format!("/{model_name}");
    let mut partials: Vec<Box<dyn PartialElement<ConformanceInstance>>> = Vec::new();
    let mut attribute_events = inherited_attribute_events.clone();
    attribute_events.extend(model_attribute_event_names(model_object, &model_root)?);
    let guard_ids = case_guard_behavior_ids(case)?;

    let mut behavior_ids: Vec<_> = case.behaviors.keys().cloned().collect();
    behavior_ids.sort();
    let mut declared_operation_ids = BTreeSet::new();
    for behavior_id in behavior_ids {
        declared_operation_ids.insert(behavior_id.clone());
        if guard_ids.contains(&behavior_id) {
            partials.push(guard_operation(&behavior_id, conformance_guard));
        } else {
            partials.push(Operation(&behavior_id, conformance_operation));
        }
    }

    let mut operation_ids: Vec<_> = case.operation_aliases.keys().cloned().collect();
    operation_ids.sort();
    for operation_id in operation_ids {
        if declared_operation_ids.insert(operation_id.clone()) {
            partials.push(Operation(&operation_id, conformance_operation));
        }
    }

    let mut combined_guard_ids: Vec<_> = case.combined_guards.keys().cloned().collect();
    combined_guard_ids.sort();
    for operation_id in combined_guard_ids {
        if declared_operation_ids.insert(operation_id.clone()) {
            partials.push(guard_operation(&operation_id, conformance_guard));
        }
    }

    append_attributes(&mut partials, optional_object(model_object, "attributes")?)?;

    for point in optional_array(model_object, "entry_points")? {
        partials.push(build_entry_point(&model_root, point)?);
    }
    for point in optional_array(model_object, "exit_points")? {
        partials.push(build_exit_point(point)?);
    }

    if let Some(initial) = model_object.get("initial") {
        partials.push(build_initial(&model_root, initial)?);
    }

    for state in optional_array(model_object, "states")? {
        partials.push(build_state(
            case,
            models,
            build_stack,
            &model_root,
            &model_root,
            &attribute_events,
            state,
        )?);
    }

    for transition in optional_array(model_object, "transitions")? {
        partials.push(build_transition(
            &model_root,
            &model_root,
            &attribute_events,
            transition,
        )?);
    }

    let model = if let Some(base_model) = base_model {
        RedefineAs(&base_model, &model_name, partials)
    } else {
        Define(&model_name, partials)
    };
    build_stack.remove(&model_name);
    Ok(model)
}

fn case_guard_behavior_ids(case: &CaseData) -> ConfResult<BTreeSet<String>> {
    let mut guard_ids = BTreeSet::new();
    collect_guard_behaviors(&case.model, &mut guard_ids)?;
    for model in &case.models {
        collect_guard_behaviors(model, &mut guard_ids)?;
    }
    Ok(guard_ids)
}

fn collect_guard_behaviors(model: &Json, guard_ids: &mut BTreeSet<String>) -> ConfResult<()> {
    let model = object(model)?;
    for transition in optional_array(model, "transitions")? {
        collect_transition_guard_behavior(transition, guard_ids)?;
    }
    for state in optional_array(model, "states")? {
        collect_state_guard_behaviors(state, guard_ids)?;
    }
    Ok(())
}

fn collect_state_guard_behaviors(state: &Json, guard_ids: &mut BTreeSet<String>) -> ConfResult<()> {
    let state = object(state)?;
    for transition in optional_array(state, "transitions")? {
        collect_transition_guard_behavior(transition, guard_ids)?;
    }
    for child in optional_array(state, "states")? {
        collect_state_guard_behaviors(child, guard_ids)?;
    }
    Ok(())
}

fn collect_transition_guard_behavior(
    transition: &Json,
    guard_ids: &mut BTreeSet<String>,
) -> ConfResult<()> {
    let transition = object(transition)?;
    if let Some(guard) = transition.get("guard") {
        guard_ids.insert(behavior_ref(guard)?);
    }
    if !transition.contains_key("guard") {
        if let Some(trigger) = optional_object(transition, "trigger")? {
            if required_string(trigger, "kind")?.as_str() == "when" {
                if let Some(behavior) = optional_string(trigger, "behavior")? {
                    guard_ids.insert(behavior);
                }
            }
        }
    }
    Ok(())
}

fn build_state(
    case: &CaseData,
    models: &BTreeMap<String, Json>,
    build_stack: &mut BTreeSet<String>,
    model_root: &str,
    owner_path: &str,
    attribute_events: &AttributeEventMap,
    state_ir: &Json,
) -> ConfResult<Box<dyn PartialElement<ConformanceInstance>>> {
    let state_object = object(state_ir)?;
    let name = required_string(state_object, "name")?;
    let kind = optional_string(state_object, "kind")?.unwrap_or_else(|| "state".to_string());
    let state_path = join_path(owner_path, &name);

    if kind == "final" {
        if state_object.contains_key("entry")
            || state_object.contains_key("exit")
            || state_object.contains_key("activity")
            || state_object.contains_key("states")
            || state_object.contains_key("transitions")
            || state_object.contains_key("initial")
        {
            return Err(ConfError::Skip(
                "final states with nested behavior are not supported yet".to_string(),
            ));
        }
        return Ok(Final(&name));
    }

    if kind == "choice" {
        let mut partials: Vec<Box<dyn PartialElement<ConformanceInstance>>> = Vec::new();
        for transition in optional_array(state_object, "transitions")? {
            partials.push(build_transition(
                owner_path,
                owner_path,
                attribute_events,
                transition,
            )?);
        }
        return Ok(choice_with_transitions(&name, partials));
    }

    if kind == "shallow_history" || kind == "deep_history" {
        let transitions = optional_array(state_object, "transitions")?;
        let mut transition_partials = Vec::new();
        for transition in transitions {
            transition_partials.push(build_history_default_transition(
                owner_path,
                owner_path,
                attribute_events,
                transition,
            )?);
        }
        let history_kind = if kind == "shallow_history" {
            kind::SHALLOW_HISTORY
        } else {
            kind::DEEP_HISTORY
        };
        return Ok(Box::new(ConformanceHistory {
            name,
            kind: history_kind,
            transitions: transition_partials,
        }));
    }

    if kind == "submachine" {
        let machine = required_string(state_object, "machine")?;
        let Some(child_ir) = models.get(&machine) else {
            return Err(ConfError::Fail(format!(
                "missing_submachine_model: missing submachine model \"{machine}\""
            )));
        };
        let child_model =
            build_model_ir_with_attributes(case, child_ir, models, build_stack, attribute_events)?;
        let mut partials: Vec<Box<dyn PartialElement<ConformanceInstance>>> = Vec::new();
        for ref_ir in optional_array(state_object, "entry")? {
            partials.push(entry_operation(&behavior_ref(ref_ir)?));
        }
        for ref_ir in optional_array(state_object, "exit")? {
            partials.push(exit_operation(&behavior_ref(ref_ir)?));
        }
        for ref_ir in optional_array(state_object, "activity")? {
            partials.push(activity_operation(&behavior_ref(ref_ir)?));
        }
        append_deferred_events(&mut partials, state_object)?;
        for transition in optional_array(state_object, "transitions")? {
            partials.push(build_transition(
                model_root,
                &state_path,
                attribute_events,
                transition,
            )?);
        }
        return Ok(SubmachineState(&name, child_model, partials));
    }

    if kind != "state" {
        return Err(ConfError::Skip(format!(
            "unsupported state kind \"{kind}\""
        )));
    }

    let mut partials: Vec<Box<dyn PartialElement<ConformanceInstance>>> = Vec::new();
    if let Some(initial) = state_object.get("initial") {
        partials.push(build_initial(&state_path, initial)?);
    }

    for ref_ir in optional_array(state_object, "entry")? {
        partials.push(entry_operation(&behavior_ref(ref_ir)?));
    }
    for ref_ir in optional_array(state_object, "exit")? {
        partials.push(exit_operation(&behavior_ref(ref_ir)?));
    }
    for ref_ir in optional_array(state_object, "activity")? {
        partials.push(activity_operation(&behavior_ref(ref_ir)?));
    }
    append_deferred_events(&mut partials, state_object)?;
    for child in optional_array(state_object, "states")? {
        partials.push(build_state(
            case,
            models,
            build_stack,
            model_root,
            &state_path,
            attribute_events,
            child,
        )?);
    }
    for transition in optional_array(state_object, "transitions")? {
        partials.push(build_transition(
            model_root,
            &state_path,
            attribute_events,
            transition,
        )?);
    }

    Ok(State(&name, partials))
}

fn build_initial(
    owner_path: &str,
    initial_ir: &Json,
) -> ConfResult<Box<dyn PartialElement<ConformanceInstance>>> {
    let target = match initial_ir {
        Json::String(value) => resolve_initial_path(owner_path, value),
        Json::Object(object) => {
            let target = resolve_initial_path(owner_path, &required_string(object, "target")?);
            let mut partials: Vec<Box<dyn PartialElement<ConformanceInstance>>> =
                vec![Target(&target)];
            for ref_ir in optional_array(object, "effects")? {
                partials.push(effect_operation(&behavior_ref(ref_ir)?));
            }
            return Ok(Initial(partials));
        }
        _ => {
            return Err(ConfError::Fail(
                "initial must be a string or object".to_string(),
            ));
        }
    };

    Ok(Initial(vec![Target(&target)]))
}

fn build_entry_point(
    model_root: &str,
    point_ir: &Json,
) -> ConfResult<Box<dyn PartialElement<ConformanceInstance>>> {
    let point = object(point_ir)?;
    let name = required_string(point, "name")?;
    let target = required_string(point, "target")?;
    let mut partials: Vec<Box<dyn PartialElement<ConformanceInstance>>> = vec![Target(
        &resolve_transition_path(model_root, model_root, &target),
    )];
    for ref_ir in optional_array(point, "effects")? {
        partials.push(effect_operation(&behavior_ref(ref_ir)?));
    }
    Ok(EntryPoint(&name, partials))
}

fn build_exit_point(point_ir: &Json) -> ConfResult<Box<dyn PartialElement<ConformanceInstance>>> {
    let point = object(point_ir)?;
    let name = required_string(point, "name")?;
    let mut partials: Vec<Box<dyn PartialElement<ConformanceInstance>>> = Vec::new();
    for ref_ir in optional_array(point, "effects")? {
        partials.push(effect_operation(&behavior_ref(ref_ir)?));
    }
    Ok(ExitPoint(&name, partials))
}

fn append_deferred_events(
    partials: &mut Vec<Box<dyn PartialElement<ConformanceInstance>>>,
    state: &BTreeMap<String, Json>,
) -> ConfResult<()> {
    let events = optional_array(state, "defer")?
        .into_iter()
        .map(event_name)
        .collect::<ConfResult<Vec<_>>>()?;
    if !events.is_empty() {
        partials.push(Defer(events));
    }
    Ok(())
}

struct ConformanceTimer {
    kind: ConformanceTimerKind,
    source: TimerSource,
}

impl PartialElement<ConformanceInstance> for ConformanceTimer {
    fn apply(self: Box<Self>, model: &mut Model<ConformanceInstance>, stack: &mut Vec<String>) {
        let transition_qn = stack.last().unwrap().clone();
        let constraint_name =
            stateforward_hsm::path::join(&transition_qn, self.kind.constraint_name());
        let event_name = stateforward_hsm::path::join(
            &transition_qn,
            if self.kind.is_timepoint() {
                "timepoint"
            } else {
                "duration"
            },
        );
        let constraint = Constraint {
            element: NamedElement {
                kind: kind::CONSTRAINT,
                qualified_name: constraint_name.clone(),
            },
            guard: None,
            operation: None,
            duration: (!self.kind.is_timepoint()).then_some(conformance_timer_duration),
            timepoint: self
                .kind
                .is_timepoint()
                .then_some(conformance_timer_timepoint),
        };
        model.set_member(
            constraint_name.clone(),
            ElementVariant::Constraint(constraint),
        );
        if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&transition_qn)
        {
            transition.events.push(event_name.clone());
            if transition.guard.is_empty() {
                transition.guard = constraint_name;
            }
        }
        register_timer_spec(
            event_name,
            TimerSpec {
                source: self.source,
            },
        );
    }
}

fn build_history_default_transition(
    target_root: &str,
    owner_path: &str,
    _attribute_events: &AttributeEventMap,
    transition_ir: &Json,
) -> ConfResult<Vec<Box<dyn PartialElement<ConformanceInstance>>>> {
    let transition_object = object(transition_ir)?;
    if transition_object.contains_key("on") || transition_object.contains_key("trigger") {
        return Err(ConfError::Skip(
            "history default triggers are not supported yet".to_string(),
        ));
    }
    if transition_object.contains_key("source") {
        return Err(ConfError::Skip(
            "history default source overrides are not supported yet".to_string(),
        ));
    }

    let mut partials: Vec<Box<dyn PartialElement<ConformanceInstance>>> = Vec::new();
    if let Some(guard) = transition_object.get("guard") {
        partials.push(guard_operation_ref(&behavior_ref(guard)?));
    }
    let Some(target) = optional_string(transition_object, "target")? else {
        return Err(ConfError::Skip(
            "history default transition requires a target".to_string(),
        ));
    };
    partials.push(Target(&resolve_transition_target_path(
        target_root,
        owner_path,
        None,
        &target,
    )));
    for ref_ir in optional_array(transition_object, "effects")? {
        partials.push(effect_operation(&behavior_ref(ref_ir)?));
    }
    Ok(partials)
}

fn push_attribute_trigger(
    partials: &mut Vec<Box<dyn PartialElement<ConformanceInstance>>>,
    attribute_events: &AttributeEventMap,
    attribute: &str,
) {
    if attribute.starts_with('/') {
        partials.push(Box::new(PartialTrigger {
            events: vec![attribute.to_string()],
        }));
    } else if let Some(event) = attribute_events.get(attribute) {
        partials.push(Box::new(PartialTrigger {
            events: vec![event.clone()],
        }));
    } else {
        partials.push(OnSet(attribute));
    }
}

fn build_transition(
    model_root: &str,
    owner_path: &str,
    attribute_events: &AttributeEventMap,
    transition_ir: &Json,
) -> ConfResult<Box<dyn PartialElement<ConformanceInstance>>> {
    let transition_object = object(transition_ir)?;
    let transition_kind = optional_string(transition_object, "kind")?;

    let mut partials: Vec<Box<dyn PartialElement<ConformanceInstance>>> = Vec::new();
    if let Some(kind_name) = &transition_kind {
        partials.push(Box::new(ConformanceTransitionKind {
            kind: transition_kind_value(kind_name)?,
        }));
    }
    let mut exit_point_trigger = None;
    let guard_ref = transition_object
        .get("guard")
        .map(behavior_ref)
        .transpose()?;
    let mut guard_consumed = false;
    if let Some(on) = transition_object.get("on") {
        partials.push(On(&event_name(on)?));
    } else if let Some(trigger) = transition_object.get("trigger") {
        let trigger_object = object(trigger)?;
        let trigger_kind = required_string(trigger_object, "kind")?;
        match trigger_kind.as_str() {
            "on" => {
                partials.push(Box::new(PartialTrigger {
                    events: event_names_from_on_trigger(trigger_object)?,
                }));
            }
            "on_set" => push_attribute_trigger(
                &mut partials,
                attribute_events,
                &required_string(trigger_object, "attribute")?,
            ),
            "on_call" => partials.push(OnCall(&required_string(trigger_object, "operation")?)),
            "after" => partials.push(Box::new(ConformanceTimer {
                kind: ConformanceTimerKind::After,
                source: timer_source_from_trigger(trigger_object, "duration_ms")?,
            })),
            "every" => partials.push(Box::new(ConformanceTimer {
                kind: ConformanceTimerKind::Every,
                source: timer_source_from_trigger(trigger_object, "duration_ms")?,
            })),
            "at" => partials.push(Box::new(ConformanceTimer {
                kind: ConformanceTimerKind::At,
                source: timer_source_from_trigger(trigger_object, "time_ms")?,
            })),
            "completion" => partials.push(On("hsm/final")),
            "exit_point" => {
                exit_point_trigger = Some(required_string(trigger_object, "exit_point")?);
            }
            "when" => {
                if let Some(attribute) = optional_string(trigger_object, "attribute")? {
                    push_attribute_trigger(&mut partials, attribute_events, &attribute);
                } else if let Some(when_behavior) = optional_string(trigger_object, "behavior")? {
                    let events = attribute_events.values().cloned().collect::<Vec<_>>();
                    if events.is_empty() {
                        return Err(ConfError::Skip(
                            "when behavior trigger requires at least one model attribute"
                                .to_string(),
                        ));
                    }
                    partials.push(Box::new(PartialTrigger { events }));
                    let guard_operation = guard_ref
                        .as_ref()
                        .map(|guard_behavior| combined_guard_name(&when_behavior, guard_behavior))
                        .unwrap_or(when_behavior);
                    partials.push(guard_operation_ref(&guard_operation));
                    guard_consumed = true;
                } else {
                    return Err(ConfError::Skip(
                        "when trigger requires attribute or behavior".to_string(),
                    ));
                }
            }
            _ => {
                return Err(ConfError::Skip(format!(
                    "unsupported trigger kind \"{trigger_kind}\""
                )));
            }
        }
    }

    let resolved_source = optional_string(transition_object, "source")?
        .map(|source| resolve_transition_path(model_root, owner_path, &source));
    if let Some(source) = &resolved_source {
        partials.push(Source(source));
    }
    if let Some(exit_point) = exit_point_trigger {
        partials.push(ExitPoint(&exit_point, Vec::new()));
    }
    if let Some(guard) = &guard_ref {
        if !guard_consumed {
            partials.push(guard_operation_ref(guard));
        }
    }
    if let Some(target) = optional_string(transition_object, "target")? {
        partials.push(Target(&resolve_transition_target_path(
            model_root,
            owner_path,
            resolved_source.as_deref(),
            &target,
        )));
    }
    if let Some(entry_point) = optional_string(transition_object, "entry_point")? {
        partials.push(EntryPoint(&entry_point, Vec::new()));
    }
    for ref_ir in optional_array(transition_object, "effects")? {
        partials.push(effect_operation(&behavior_ref(ref_ir)?));
    }

    Ok(Transition(partials))
}

fn timer_source_from_trigger(
    trigger: &BTreeMap<String, Json>,
    literal_field: &str,
) -> ConfResult<TimerSource> {
    if let Some(value) = trigger.get(literal_field) {
        let millis = json_u64(value, literal_field)?;
        return Ok(if literal_field == "time_ms" {
            TimerSource::TimeMs(millis)
        } else {
            TimerSource::DurationMs(millis)
        });
    }
    if let Some(attribute) = optional_string(trigger, "attribute")? {
        return Ok(TimerSource::Attribute(attribute));
    }
    if let Some(behavior) = optional_string(trigger, "behavior")? {
        return Ok(TimerSource::Behavior(behavior));
    }
    if let Some(value) = trigger.get("time_ms") {
        return Ok(TimerSource::TimeMs(json_u64(value, "time_ms")?));
    }
    Err(ConfError::Fail("timer trigger missing source".to_string()))
}

async fn execute_step(
    ctx: &Context,
    hsm: &HSM<ConformanceInstance>,
    groups: &BTreeMap<String, Group<ConformanceInstance>>,
    clock: &Arc<LogicalClock>,
    step_ir: &Json,
    trace_steps: StepTraceOptions,
) -> ConfResult<Option<String>> {
    let step_object = object(step_ir)?;
    let op = required_string(step_object, "op")?;
    match op.as_str() {
        "start" => {
            if trace_steps.start {
                push_step_trace(hsm, "start");
            }
            if hsm.instance().read().unwrap().has_issue() {
                return Ok(None);
            }
            {
                let instance = hsm.instance().read().unwrap();
                clear_deferred_events_for_instance(
                    &mut instance.defer_trace.lock().unwrap(),
                    &hsm.id(),
                );
            }
            hsm.start().await?;
            Ok(None)
        }
        "restart" => {
            if trace_steps.restart {
                push_step_trace(hsm, "restart");
            }
            cancel_instance_timers(hsm, clock, trace_steps, None);
            {
                let instance = hsm.instance().read().unwrap();
                clear_deferred_events_for_instance(
                    &mut instance.defer_trace.lock().unwrap(),
                    &hsm.id(),
                );
            }
            Restart(ctx, hsm).await?;
            Ok(None)
        }
        "stop" => {
            if trace_steps.stop {
                push_step_trace(hsm, "stop");
            }
            cancel_instance_timers(hsm, clock, trace_steps, None);
            {
                let instance = hsm.instance().read().unwrap();
                clear_deferred_events_for_instance(
                    &mut instance.defer_trace.lock().unwrap(),
                    &hsm.id(),
                );
            }
            Stop(ctx, hsm).await?;
            Ok(None)
        }
        "tick" => {
            let millis = optional_number_u64(step_object, "millis")?.unwrap_or(0);
            clock.advance(millis).await;
            Ok(None)
        }
        "sleep" => {
            let millis = optional_number_u64(step_object, "millis")?.unwrap_or(0);
            clock.advance(millis).await;
            Ok(None)
        }
        "dispatch" => {
            let event_ir = step_object
                .get("event")
                .ok_or_else(|| ConfError::Fail("dispatch step missing event".to_string()))?;
            let event_name = event_name(event_ir)?;
            let event = event_from_json(event_ir)?;
            {
                let instance = hsm.instance().read().unwrap();
                instance.push_trace(Json::object(vec![
                    ("type", Json::String("dispatch".to_string())),
                    ("event", Json::String(event_name)),
                ]));
            }
            let before_state = hsm.state();
            let cancel_insert_at = trace_len(hsm);
            let event_name = event.name.clone();
            let record = hsm_record(hsm);
            let traced = {
                let instance = hsm.instance().read().unwrap();
                let records = vec![record.clone()];
                let traced = instance.trace_deferred_dispatch_records(&event_name, &records);
                instance.trace_undefer_before_dispatch(&record.0, &record.1, &event_name);
                traced
            };
            hsm.dispatch(ctx, event).await?;
            {
                let instance = hsm.instance().read().unwrap();
                let records = vec![hsm_record(hsm)];
                instance.trace_runtime_deferred_records(&event_name, &records, &traced);
            }
            if hsm.state() != before_state {
                remove_cancelled_instance_timers(hsm, clock, trace_steps, Some(cancel_insert_at));
            }
            Ok(None)
        }
        "dispatch_to" => {
            let event_ir = step_object
                .get("event")
                .ok_or_else(|| ConfError::Fail("dispatch_to step missing event".to_string()))?;
            let event_name = event_name(event_ir)?;
            let event = event_from_json(event_ir)?;
            let targets = dispatch_to_step_targets(step_object)?;
            {
                let instance = hsm.instance().read().unwrap();
                instance.push_trace(Json::object(vec![
                    ("type", Json::String("dispatch".to_string())),
                    ("event", Json::String(event_name)),
                    ("target", targets.trace_target),
                ]));
            }
            let cancel_insert_at = trace_len(hsm);
            let event_name = event.name.clone();
            let before_records = records_for_ids(ctx, &targets.ids);
            let traced = {
                let instance = hsm.instance().read().unwrap();
                instance.trace_deferred_dispatch_records(&event_name, &before_records)
            };
            dispatch_to_targets(ctx, event, &targets.ids).await?;
            {
                let instance = hsm.instance().read().unwrap();
                let after_records = records_for_ids(ctx, &targets.ids);
                instance.trace_runtime_deferred_records(&event_name, &after_records, &traced);
                for id in changed_instance_ids(&before_records, &after_records) {
                    remove_cancelled_timers_for_id(
                        hsm,
                        &id,
                        clock,
                        trace_steps,
                        Some(cancel_insert_at),
                    );
                }
            }
            Ok(Some(targets.stable_label))
        }
        "dispatch_all" => {
            let event_ir = step_object
                .get("event")
                .ok_or_else(|| ConfError::Fail("dispatch_all step missing event".to_string()))?;
            let event_name = event_name(event_ir)?;
            let event = event_from_json(event_ir)?;
            {
                let instance = hsm.instance().read().unwrap();
                instance.push_trace(Json::object(vec![
                    ("type", Json::String("dispatch".to_string())),
                    ("event", Json::String(event_name)),
                    ("target", Json::String("all".to_string())),
                ]));
            }
            let cancel_insert_at = trace_len(hsm);
            let event_name = event.name.clone();
            let before_records = context_machine_records(ctx);
            let traced = {
                let instance = hsm.instance().read().unwrap();
                instance.trace_deferred_dispatch_records(&event_name, &before_records)
            };
            dispatch_all_with_route_targets(ctx, event, None).await?;
            {
                let instance = hsm.instance().read().unwrap();
                let after_records = context_machine_records(ctx);
                instance.trace_runtime_deferred_records(&event_name, &after_records, &traced);
                for id in changed_instance_ids(&before_records, &after_records) {
                    remove_cancelled_timers_for_id(
                        hsm,
                        &id,
                        clock,
                        trace_steps,
                        Some(cancel_insert_at),
                    );
                }
            }
            Ok(Some("all".to_string()))
        }
        "group_dispatch" => {
            let event_ir = step_object
                .get("event")
                .ok_or_else(|| ConfError::Fail("group_dispatch step missing event".to_string()))?;
            let event_name = event_name(event_ir)?;
            let event = event_from_json(event_ir)?;
            let group_id = required_string(step_object, "group")?;
            {
                let instance = hsm.instance().read().unwrap();
                instance.push_trace(Json::object(vec![
                    ("type", Json::String("dispatch".to_string())),
                    ("event", Json::String(event_name)),
                    ("target", Json::String(group_id.clone())),
                ]));
            }
            let cancel_insert_at = trace_len(hsm);
            let group = groups
                .get(&group_id)
                .ok_or_else(|| ConfError::Fail(format!("unknown group \"{group_id}\"")))?;
            let event_name = event.name.clone();
            let before_records = records_for_group(group);
            let traced = {
                let instance = hsm.instance().read().unwrap();
                instance.trace_deferred_dispatch_records(&event_name, &before_records)
            };
            group.dispatch(ctx, event).await?;
            {
                let instance = hsm.instance().read().unwrap();
                let after_records = records_for_group(group);
                instance.trace_runtime_deferred_records(&event_name, &after_records, &traced);
                for id in changed_instance_ids(&before_records, &after_records) {
                    remove_cancelled_timers_for_id(
                        hsm,
                        &id,
                        clock,
                        trace_steps,
                        Some(cancel_insert_at),
                    );
                }
            }
            Ok(Some(format!("group:{group_id}")))
        }
        "call" => {
            let operation = required_string(step_object, "operation")?;
            {
                let instance = hsm.instance().read().unwrap();
                instance.push_trace(Json::object(vec![
                    ("type", Json::String("call".to_string())),
                    ("operation", Json::String(operation.clone())),
                ]));
            }
            hsm.call_with_args(ctx, &operation, call_args_from_step(step_object)?)
                .await?;
            Ok(None)
        }
        "expect" => {
            let Json::Object(expect) = step_object
                .get("expect")
                .ok_or_else(|| ConfError::Fail("expect step missing expect object".to_string()))?
            else {
                return Err(ConfError::Fail(
                    "expect step must contain an object".to_string(),
                ));
            };
            let mut hsms = BTreeMap::new();
            hsms.insert("default".to_string(), hsm.clone());
            assert_expectations(expect, &hsms, None)?;
            Ok(None)
        }
        "set" => {
            let attribute = required_string(step_object, "attribute")?;
            let value = step_object
                .get("value")
                .ok_or_else(|| ConfError::Fail("set step missing value".to_string()))?;
            if trace_steps.set {
                let instance = hsm.instance().read().unwrap();
                instance.push_trace(Json::object(vec![
                    ("type", Json::String("set".to_string())),
                    ("attribute", Json::String(attribute.clone())),
                    ("value", value.clone()),
                ]));
            }
            hsm.set(&attribute, json_to_attribute_value(value)?)?;
            Ok(None)
        }
        "snapshot" => {
            if let Some(group_id) = optional_string(step_object, "group")? {
                let id = optional_string(step_object, "id")?.unwrap_or_else(|| group_id.clone());
                let group = groups
                    .get(&group_id)
                    .ok_or_else(|| ConfError::Fail(format!("unknown group \"{group_id}\"")))?;
                record_group_snapshot(hsm, group, &group_id, &id)?;
                Ok(Some(format!("group:{group_id}")))
            } else {
                let id = optional_string(step_object, "id")?.unwrap_or_else(|| "last".to_string());
                record_snapshot(hsm, &id)?;
                Ok(None)
            }
        }
        other => Err(ConfError::Skip(format!(
            "unsupported script op \"{other}\""
        ))),
    }
}

fn push_step_trace(hsm: &HSM<ConformanceInstance>, kind: &str) {
    let instance = hsm.instance().read().unwrap();
    instance.push_trace(Json::object(vec![("type", Json::String(kind.to_string()))]));
}

fn cancel_instance_timers(
    hsm: &HSM<ConformanceInstance>,
    clock: &LogicalClock,
    trace_steps: StepTraceOptions,
    insert_at: Option<usize>,
) {
    cancel_timers_for_id(hsm, &hsm.id(), clock, trace_steps, insert_at);
}

fn remove_cancelled_instance_timers(
    hsm: &HSM<ConformanceInstance>,
    clock: &LogicalClock,
    trace_steps: StepTraceOptions,
    insert_at: Option<usize>,
) {
    remove_cancelled_timers_for_id(hsm, &hsm.id(), clock, trace_steps, insert_at);
}

fn cancel_timers_for_id(
    trace_hsm: &HSM<ConformanceInstance>,
    instance_id: &str,
    clock: &LogicalClock,
    trace_steps: StepTraceOptions,
    insert_at: Option<usize>,
) {
    if clock.cancel_for_instance(instance_id) == 0 || !trace_steps.timer_cancelled {
        return;
    }
    push_timer_cancelled_trace(trace_hsm, insert_at);
}

fn remove_cancelled_timers_for_id(
    trace_hsm: &HSM<ConformanceInstance>,
    instance_id: &str,
    clock: &LogicalClock,
    trace_steps: StepTraceOptions,
    insert_at: Option<usize>,
) {
    if clock.remove_cancelled_for_instance(instance_id) == 0 || !trace_steps.timer_cancelled {
        return;
    }
    push_timer_cancelled_trace(trace_hsm, insert_at);
}

fn push_timer_cancelled_trace(trace_hsm: &HSM<ConformanceInstance>, insert_at: Option<usize>) {
    let instance = trace_hsm.instance().read().unwrap();
    let mut trace = instance.trace.lock().unwrap();
    let entry = Json::object(vec![("type", Json::String("timer_cancelled".to_string()))]);
    if let Some(index) = insert_at {
        let index = index.min(trace.len());
        trace.insert(index, entry);
    } else {
        trace.push(entry);
    }
}

fn trace_len(hsm: &HSM<ConformanceInstance>) -> usize {
    let instance = hsm.instance().read().unwrap();
    instance.trace.lock().unwrap().len()
}

fn behavior_target_records(
    ctx: &Context,
    target: &BehaviorDispatchTarget,
    groups: &Arc<Mutex<BTreeMap<String, Group<ConformanceInstance>>>>,
) -> Vec<(String, String)> {
    match target {
        BehaviorDispatchTarget::Current => {
            let (machine, ok) = FromContext::<ConformanceInstance>(ctx);
            if ok {
                machine
                    .map(|hsm| vec![(hsm.id(), hsm.state())])
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        }
        BehaviorDispatchTarget::Instance(target) => context_machine_records(ctx)
            .into_iter()
            .filter(|(id, _)| id == target)
            .collect(),
        BehaviorDispatchTarget::All => context_machine_records(ctx),
        BehaviorDispatchTarget::Group(group_id) => groups
            .lock()
            .unwrap()
            .get(group_id)
            .map(|group| {
                group
                    .machines()
                    .into_iter()
                    .map(|hsm| (hsm.id(), hsm.state()))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn context_machine_records(ctx: &Context) -> Vec<(String, String)> {
    let (machines, ok) = InstancesFromContext(ctx);
    if !ok {
        return Vec::new();
    }
    machines
        .into_iter()
        .map(|machine| (machine.id(), machine.state()))
        .collect()
}

fn records_for_ids(ctx: &Context, ids: &[String]) -> Vec<(String, String)> {
    let id_set = ids.iter().collect::<BTreeSet<_>>();
    context_machine_records(ctx)
        .into_iter()
        .filter(|(id, _)| id_set.contains(id))
        .collect()
}

fn records_for_group(group: &Group<ConformanceInstance>) -> Vec<(String, String)> {
    group
        .machines()
        .into_iter()
        .map(|hsm| (hsm.id(), hsm.state()))
        .collect()
}

fn changed_instance_ids(before: &[(String, String)], after: &[(String, String)]) -> Vec<String> {
    let after = after.iter().cloned().collect::<BTreeMap<_, _>>();
    before
        .iter()
        .filter_map(|(id, before_state)| {
            after
                .get(id)
                .is_some_and(|after_state| after_state != before_state)
                .then(|| id.clone())
        })
        .collect()
}

fn hsm_record(hsm: &HSM<ConformanceInstance>) -> (String, String) {
    (hsm.id(), hsm.state())
}

fn pop_deferred_event_for_instance(
    defer_trace: &mut DeferTraceState,
    instance_id: &str,
) -> Option<String> {
    let index = defer_trace
        .deferred_events
        .iter()
        .position(|event| event.instance_id == instance_id)?;
    Some(defer_trace.deferred_events.remove(index).event_name)
}

fn clear_deferred_events_for_instance(defer_trace: &mut DeferTraceState, instance_id: &str) {
    defer_trace
        .deferred_events
        .retain(|event| event.instance_id != instance_id);
}

fn clear_child_deferred_events_for_instance(defer_trace: &mut DeferTraceState, instance_id: &str) {
    defer_trace
        .deferred_events
        .retain(|event| event.instance_id != instance_id || !event.cleanup_on_parent_exit);
}

fn has_deferred_event(defer_trace: &DeferTraceState, instance_id: &str, event_name: &str) -> bool {
    defer_trace
        .deferred_events
        .iter()
        .any(|event| event.instance_id == instance_id && event.event_name == event_name)
}

fn note_deferred_event(
    defer_trace: &mut DeferTraceState,
    instance_id: &str,
    state: &str,
    event_name: &str,
) {
    if has_deferred_event(defer_trace, instance_id, event_name) {
        return;
    }
    let Some(owner) = event_is_deferred(defer_trace, instance_id, state, event_name) else {
        return;
    };
    let cleanup_on_parent_exit = defer_trace
        .models
        .get(instance_id)
        .map(|model| deferred_cleanup_on_parent_exit(model, &owner))
        .unwrap_or(false);
    defer_trace.deferred_events.push(DeferredTraceEvent {
        instance_id: instance_id.to_string(),
        event_name: event_name.to_string(),
        cleanup_on_parent_exit,
    });
}

fn event_is_deferred(
    defer_trace: &DeferTraceState,
    instance_id: &str,
    state: &str,
    event_name: &str,
) -> Option<String> {
    let model = defer_trace.models.get(instance_id)?;
    let root_state = model.state.qualified_name().to_string();
    let mut selection_state = state.to_string();
    let mut unguarded_candidate_seen = false;

    loop {
        if has_transition_candidate_at_state(model, state, &selection_state, event_name, true) {
            unguarded_candidate_seen = true;
        }
        if state_declares_defer(model, &selection_state, event_name) {
            return (!unguarded_candidate_seen).then_some(selection_state);
        }
        if selection_state == root_state {
            break;
        }
        let parent = dirname(&selection_state);
        if parent == selection_state || parent == "/" {
            break;
        }
        selection_state = parent.to_string();
    }

    None
}

fn event_has_transition_candidate(
    defer_trace: &DeferTraceState,
    instance_id: &str,
    state: &str,
    event_name: &str,
) -> bool {
    let Some(model) = defer_trace.models.get(instance_id) else {
        return false;
    };
    let root_state = model.state.qualified_name().to_string();
    let mut selection_state = state.to_string();

    loop {
        if has_transition_candidate_at_state(model, state, &selection_state, event_name, false) {
            return true;
        }
        if state_declares_defer(model, &selection_state, event_name) {
            return false;
        }
        if selection_state == root_state {
            break;
        }
        let parent = dirname(&selection_state);
        if parent == selection_state || parent == "/" {
            break;
        }
        selection_state = parent.to_string();
    }

    false
}

fn has_transition_candidate_at_state(
    model: &Model<ConformanceInstance>,
    active_state: &str,
    selection_state: &str,
    event_name: &str,
    require_unguarded: bool,
) -> bool {
    let Some(transitions_by_event) = model.transition_map.get(active_state) else {
        return false;
    };
    for lookup_name in trace_lookup_names(event_name) {
        let Some(transition_names) = transitions_by_event.get(&lookup_name) else {
            continue;
        };
        for transition_name in transition_names {
            let Some(transition) = model.get_transition(transition_name) else {
                continue;
            };
            if transition_source_matches_selection(model, transition, selection_state, event_name)
                && (!require_unguarded || transition.guard.is_empty())
            {
                return true;
            }
        }
    }
    false
}

fn transition_source_matches_selection(
    model: &Model<ConformanceInstance>,
    transition: &stateforward_hsm::element::Transition,
    selection_state: &str,
    event_name: &str,
) -> bool {
    if transition.source == selection_state {
        let root_state = model.state.qualified_name();
        let transition_owner = dirname(transition.qualified_name());
        let handles_at_or_below = transition_owner == selection_state
            || dirname(selection_state) == root_state
            || stateforward_hsm::path::is_ancestor_or_equal(selection_state, transition_owner);
        return handles_at_or_below || !state_declares_defer(model, selection_state, event_name);
    }

    let selection_initial = if selection_state == model.state.qualified_name() {
        model.state.initial.as_str()
    } else {
        model
            .get_state(selection_state)
            .map(|state| state.initial.as_str())
            .unwrap_or("")
    };
    selection_initial == transition.source
}

fn state_declares_defer(model: &Model<ConformanceInstance>, state: &str, event_name: &str) -> bool {
    let deferred = if state == model.state.qualified_name() {
        &model.state.deferred
    } else {
        let Some(state) = model.get_state(state) else {
            return false;
        };
        &state.deferred
    };
    deferred
        .iter()
        .any(|deferred| deferred == event_name || deferred == ANY_EVENT_NAME)
}

fn behavior_owner_defer_state(
    model: &Model<ConformanceInstance>,
    behavior_id: &str,
    event_name: &str,
) -> Option<String> {
    for member in model.members.values() {
        let ElementVariant::Behavior(behavior) = member else {
            continue;
        };
        let Some(operation_name) = behavior
            .operation
            .as_ref()
            .and_then(|operation| operation.operation_name())
        else {
            continue;
        };
        if basename(operation_name) != behavior_id {
            continue;
        }
        let owner = dirname(behavior.qualified_name()).to_string();
        if state_declares_defer(model, &owner, event_name) {
            return Some(owner);
        }
    }
    None
}

fn trace_lookup_names(event_name: &str) -> Vec<String> {
    let mut names = vec![event_name.to_string()];
    if event_name != ANY_EVENT_NAME {
        names.push(ANY_EVENT_NAME.to_string());
    }
    names
}

fn deferred_cleanup_on_parent_exit(model: &Model<ConformanceInstance>, owner: &str) -> bool {
    let root_state = model.state.qualified_name();
    let mut current = dirname(owner).to_string();
    while !current.is_empty() && current != "/" && current != root_state {
        if model
            .get_state(&current)
            .is_some_and(|state| is_kind(state.kind(), kind::SUBMACHINE_STATE))
        {
            return true;
        }
        let parent = dirname(&current);
        if parent == current {
            break;
        }
        current = parent.to_string();
    }
    false
}

fn event_exits_active_submachine(
    defer_trace: &DeferTraceState,
    instance_id: &str,
    state: &str,
    event_name: &str,
) -> bool {
    let Some(model) = defer_trace.models.get(instance_id) else {
        return false;
    };
    let root_state = model.state.qualified_name().to_string();
    let mut current = state.to_string();
    loop {
        if model.get_state(&current).is_some_and(|state| {
            is_kind(state.kind(), kind::SUBMACHINE_STATE)
                && state.vertex.transitions.iter().any(|transition_name| {
                    model
                        .get_transition(transition_name)
                        .is_some_and(|transition| {
                            !transition.target.is_empty()
                                && !stateforward_hsm::path::is_ancestor_or_equal(
                                    &current,
                                    &transition.target,
                                )
                                && transition
                                    .events
                                    .iter()
                                    .any(|event| event == event_name || event == ANY_EVENT_NAME)
                        })
                })
        }) {
            return true;
        }
        if current == root_state {
            break;
        }
        let parent = dirname(&current);
        if parent == current || parent == "/" {
            break;
        }
        current = parent.to_string();
    }
    false
}

async fn dispatch_to_targets(ctx: &Context, event: Event, ids: &[String]) -> ConfResult<()> {
    dispatch_to_targets_with_source(ctx, event, ids, None).await
}

async fn dispatch_to_targets_with_source(
    ctx: &Context,
    event: Event,
    ids: &[String],
    source: Option<String>,
) -> ConfResult<()> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if seen.insert(id.clone()) {
            let routed = event_with_route(&event, source.as_deref(), Some(id));
            DispatchTo(ctx, routed, vec![id.clone()]).await?;
        }
    }
    Ok(())
}

async fn dispatch_all_with_route_targets(
    ctx: &Context,
    event: Event,
    source: Option<String>,
) -> ConfResult<()> {
    let (machines, ok) = InstancesFromContext(ctx);
    if !ok {
        DispatchAll(ctx, event).await?;
        return Ok(());
    }
    let ids = machines
        .iter()
        .map(|machine| machine.id())
        .collect::<Vec<_>>();
    dispatch_to_targets_with_source(ctx, event, &ids, source).await
}

async fn drain_pending_group_dispatches(
    pending: &Arc<Mutex<VecDeque<PendingGroupDispatch>>>,
    groups: &BTreeMap<String, Group<ConformanceInstance>>,
) -> ConfResult<()> {
    loop {
        let Some(dispatch) = ({ pending.lock().unwrap().pop_front() }) else {
            return Ok(());
        };
        let group = groups
            .get(&dispatch.group_id)
            .ok_or_else(|| ConfError::Fail(format!("unknown group \"{}\"", dispatch.group_id)))?;
        group.dispatch(&dispatch.ctx, dispatch.event).await?;
    }
}

fn dispatch_to_step_targets(step: &BTreeMap<String, Json>) -> ConfResult<DispatchTargets> {
    if let Some(targets) = step.get("targets") {
        let Json::Array(targets) = targets else {
            return Err(ConfError::Fail(
                "dispatch_to step targets must be an array".to_string(),
            ));
        };
        let ids = targets.iter().map(string).collect::<ConfResult<Vec<_>>>()?;
        return Ok(DispatchTargets {
            stable_label: format!("targets:{}", ids.join(",")),
            trace_target: Json::Array(ids.iter().cloned().map(Json::String).collect()),
            ids,
        });
    }
    if let Some(target) = optional_string(step, "instance")? {
        return Ok(DispatchTargets {
            ids: vec![target.clone()],
            trace_target: Json::String(target.clone()),
            stable_label: target,
        });
    }
    if let Some(target) = optional_string(step, "target")? {
        return Ok(DispatchTargets {
            ids: vec![target.clone()],
            trace_target: Json::String(target.clone()),
            stable_label: target,
        });
    }
    Err(ConfError::Fail(
        "dispatch_to step missing target".to_string(),
    ))
}

fn assert_expectations(
    expect: &BTreeMap<String, Json>,
    hsms: &BTreeMap<String, HSM<ConformanceInstance>>,
    last_error: Option<&(String, String)>,
) -> ConfResult<()> {
    let primary_hsm = hsm_by_id(hsms, &primary_instance_id(hsms)?)?;

    if let Some(expected_error) = optional_object(expect, "error")? {
        let Some((actual_code, actual_message)) = last_error else {
            return Err(ConfError::Fail(
                "expected error but none was recorded".to_string(),
            ));
        };
        if let Some(code) = optional_string(expected_error, "code")? {
            if &code != actual_code {
                return Err(ConfError::Fail(format!(
                    "error code mismatch: expected {code}, got {actual_code}"
                )));
            }
        }
        if let Some(contains) = optional_string(expected_error, "message_contains")? {
            if !actual_message.contains(&contains) {
                return Err(ConfError::Fail(format!(
                    "error message mismatch: expected containing {contains:?}, got {actual_message:?}"
                )));
            }
        }
    } else if let Some((actual_code, actual_message)) = last_error {
        return Err(ConfError::Fail(format!(
            "unexpected error {actual_code}: {actual_message}"
        )));
    }

    if let Some(expected_state) = expect.get("state") {
        let Json::String(expected_state) = expected_state else {
            return Err(ConfError::Fail("expect.state must be a string".to_string()));
        };
        let actual_state = primary_hsm.state();
        if &actual_state != expected_state {
            return Err(ConfError::Fail(format!(
                "state mismatch: expected {expected_state}, got {actual_state}"
            )));
        }
    }

    if let Some(expected_states) = expect.get("states") {
        let expected_states = object(expected_states)?;
        for (id, expected_state) in expected_states {
            let Json::String(expected_state) = expected_state else {
                return Err(ConfError::Fail(format!(
                    "expect.states.{id} must be a string"
                )));
            };
            let actual_state = hsm_by_id(hsms, id)?.state();
            if &actual_state != expected_state {
                return Err(ConfError::Fail(format!(
                    "states.{id} mismatch: expected {expected_state}, got {actual_state}"
                )));
            }
        }
    }

    if let Some(expected_trace) = expect.get("trace") {
        let actual_trace = {
            let instance = primary_hsm.instance().read().unwrap();
            Json::Array(instance.trace())
        };
        if &actual_trace != expected_trace {
            return Err(ConfError::Fail(format!(
                "trace mismatch: expected {}, got {}",
                format_json(expected_trace),
                format_json(&actual_trace)
            )));
        }
    }

    if let Some(expected_attributes) = expect.get("attributes") {
        let actual_attributes = actual_attributes(primary_hsm, expected_attributes)?;
        assert_partial_json(&actual_attributes, expected_attributes, "attributes")?;
    }

    if let Some(expected_instances) = expect.get("instance_attributes") {
        let expected_instances = object(expected_instances)?;
        for (id, expected_attributes) in expected_instances {
            let actual_attributes = actual_attributes(hsm_by_id(hsms, id)?, expected_attributes)?;
            assert_partial_json(
                &actual_attributes,
                expected_attributes,
                &format!("instance_attributes.{id}"),
            )?;
        }
    }

    if let Some(expected_snapshots) = expect.get("snapshots") {
        let actual_snapshots = normalize_snapshot_expectation_json(aggregate_snapshots(hsms));
        let expected_snapshots = normalize_snapshot_expectation_json(expected_snapshots.clone());
        assert_partial_json(&actual_snapshots, &expected_snapshots, "snapshots")?;
    }

    Ok(())
}

fn aggregate_snapshots(hsms: &BTreeMap<String, HSM<ConformanceInstance>>) -> Json {
    let mut snapshots = BTreeMap::new();
    for hsm in hsms.values() {
        let instance = hsm.instance().read().unwrap();
        snapshots.extend(instance.snapshots());
    }
    Json::Object(snapshots)
}

fn normalize_snapshot_expectation_json(mut value: Json) -> Json {
    normalize_snapshot_expectation_value(&mut value);
    value
}

fn normalize_snapshot_expectation_value(value: &mut Json) {
    match value {
        Json::Object(object) => {
            if let Some(Json::Array(transitions)) = object.get_mut("transitions") {
                for transition in transitions.iter_mut() {
                    normalize_snapshot_transition_name(transition);
                }
                transitions.sort_by(compare_snapshot_transitions);
            }
            for child in object.values_mut() {
                normalize_snapshot_expectation_value(child);
            }
        }
        Json::Array(values) => {
            for child in values {
                normalize_snapshot_expectation_value(child);
            }
        }
        _ => {}
    }
}

fn normalize_snapshot_transition_name(transition: &mut Json) {
    let Json::Object(object) = transition else {
        return;
    };
    let Some(Json::String(name)) = object.get_mut("name") else {
        return;
    };
    if let Some(index) = name.rfind("/transition_") {
        name.truncate(index + "/transition".len());
    }
}

fn compare_snapshot_transitions(left: &Json, right: &Json) -> std::cmp::Ordering {
    let left_source = json_object_string(left, "source").unwrap_or_default();
    let right_source = json_object_string(right, "source").unwrap_or_default();
    let left_depth = left_source.matches('/').count();
    let right_depth = right_source.matches('/').count();
    right_depth
        .cmp(&left_depth)
        .then_with(|| left_source.cmp(&right_source))
        .then_with(|| {
            json_object_string(left, "name")
                .unwrap_or_default()
                .cmp(&json_object_string(right, "name").unwrap_or_default())
        })
}

fn json_object_string(value: &Json, key: &str) -> Option<String> {
    let Json::Object(object) = value else {
        return None;
    };
    let Json::String(value) = object.get(key)? else {
        return None;
    };
    Some(value.clone())
}

fn take_runtime_issue(hsm: &HSM<ConformanceInstance>) -> ConfResult<()> {
    let issue = hsm.instance().read().unwrap().take_issue();
    match issue {
        Some(RuntimeIssue {
            skip: true,
            message,
            ..
        }) => Err(ConfError::Skip(message)),
        Some(RuntimeIssue {
            code: Some(code),
            message,
            ..
        }) => Err(ConfError::Fail(format!("{code}\0{message}"))),
        Some(RuntimeIssue { message, .. }) => Err(ConfError::Fail(message)),
        None => Ok(()),
    }
}

struct ConformanceTransitionKind {
    kind: kind::KindValue,
}

impl PartialElement<ConformanceInstance> for ConformanceTransitionKind {
    fn apply(self: Box<Self>, model: &mut Model<ConformanceInstance>, stack: &mut Vec<String>) {
        let transition_qn = stack.last().unwrap().clone();
        if let Some(ElementVariant::Transition(transition)) = model.members.get_mut(&transition_qn)
        {
            transition.kind_override = Some(self.kind);
            transition.element.kind = self.kind;
        }
    }
}

struct ConformanceHistory {
    name: String,
    kind: kind::KindValue,
    transitions: Vec<Vec<Box<dyn PartialElement<ConformanceInstance>>>>,
}

impl PartialElement<ConformanceInstance> for ConformanceHistory {
    fn apply(self: Box<Self>, model: &mut Model<ConformanceInstance>, stack: &mut Vec<String>) {
        let owner_qn = stack.last().unwrap_or(&"/".to_string()).clone();
        let history_name = stateforward_hsm::path::join(&owner_qn, &self.name);
        let vertex = Vertex {
            element: NamedElement {
                kind: self.kind,
                qualified_name: history_name.clone(),
            },
            transitions: Vec::new(),
        };
        model.set_member(history_name.clone(), ElementVariant::Vertex(vertex));

        stack.push(history_name.clone());
        for (index, partials) in self.transitions.into_iter().enumerate() {
            let transition_name =
                stateforward_hsm::path::join(&history_name, &format!("transition_{index}"));
            let transition = Transition {
                element: NamedElement {
                    kind: kind::TRANSITION,
                    qualified_name: transition_name.clone(),
                },
                kind_override: None,
                source: history_name.clone(),
                target: String::new(),
                guard: String::new(),
                effect: Vec::new(),
                events: Vec::new(),
                paths: HashMap::new(),
            };
            model.set_member(
                transition_name.clone(),
                ElementVariant::Transition(transition),
            );
            stack.push(transition_name.clone());
            for partial in partials {
                partial.apply(model, stack);
            }
            stack.pop();

            if let Some(ElementVariant::Vertex(vertex)) = model.members.get_mut(&history_name) {
                vertex.transitions.push(transition_name);
            }
        }
        stack.pop();
    }
}

struct ConformanceAttribute {
    name: String,
    value_type: Option<AttributeType>,
    default_value: Option<AttributeValue>,
}

impl PartialElement<ConformanceInstance> for ConformanceAttribute {
    fn apply(self: Box<Self>, model: &mut Model<ConformanceInstance>, stack: &mut Vec<String>) {
        let owner_qn = stack
            .last()
            .cloned()
            .unwrap_or_else(|| model.state.qualified_name().to_string());
        let qualified_name = stateforward_hsm::path::join(&owner_qn, &self.name);
        let attribute = Attribute {
            element: NamedElement {
                kind: kind::ATTRIBUTE,
                qualified_name: qualified_name.clone(),
            },
            declared_name: self.name,
            value_type: self.value_type,
            default_value: self.default_value,
        };
        model
            .attributes
            .insert(qualified_name.clone(), attribute.clone());
        model.set_member(qualified_name, ElementVariant::Attribute(attribute));
    }
}

fn append_attributes(
    partials: &mut Vec<Box<dyn PartialElement<ConformanceInstance>>>,
    attributes: Option<&BTreeMap<String, Json>>,
) -> ConfResult<()> {
    let Some(attributes) = attributes else {
        return Ok(());
    };

    for (name, spec_json) in attributes {
        let spec = object(spec_json)?;
        let declared_type = optional_string(spec, "type")?;
        let default_value = spec
            .get("default")
            .map(json_to_attribute_value)
            .transpose()?;
        if declared_type.is_none() && default_value.is_none() {
            return Err(ConfError::Fail(format!(
                "attribute \"{name}\" requires type or default"
            )));
        }
        let value_type = match declared_type.as_deref() {
            Some("any") => None,
            Some(name) => Some(attribute_type(name)?),
            None => default_value.as_ref().map(AttributeValue::value_type),
        };

        partials.push(Box::new(ConformanceAttribute {
            name: name.clone(),
            value_type,
            default_value,
        }));
    }

    Ok(())
}

fn model_attribute_event_names(
    model: &BTreeMap<String, Json>,
    model_root: &str,
) -> ConfResult<AttributeEventMap> {
    let Some(attributes) = optional_object(model, "attributes")? else {
        return Ok(AttributeEventMap::new());
    };
    Ok(attributes
        .keys()
        .map(|name| (name.clone(), join_path(model_root, name)))
        .collect())
}

fn attribute_type(name: &str) -> ConfResult<AttributeType> {
    match name {
        "number" | "integer" | "duration_ms" | "time_ms" => Ok(AttributeType::Int),
        "boolean" => Ok(AttributeType::Bool),
        "string" => Ok(AttributeType::String),
        "object" => Ok(AttributeType::Object),
        "array" => Ok(AttributeType::Array),
        "null" => Ok(AttributeType::Null),
        other => Err(ConfError::Skip(format!(
            "unsupported attribute type \"{other}\""
        ))),
    }
}

fn json_u64(value: &Json, label: &str) -> ConfResult<u64> {
    let Json::Number(value) = value else {
        return Err(ConfError::Fail(format!("{label} must be a number")));
    };
    value
        .parse::<u64>()
        .map_err(|_| ConfError::Fail(format!("{label} must be a non-negative integer")))
}

fn json_to_attribute_value(value: &Json) -> ConfResult<AttributeValue> {
    match value {
        Json::Null => Ok(AttributeValue::Null),
        Json::Bool(value) => Ok(AttributeValue::Bool(*value)),
        Json::Number(value) => value
            .parse::<i64>()
            .map(AttributeValue::Int)
            .map_err(|_| ConfError::Skip(format!("unsupported non-integer number {value}"))),
        Json::String(value) => Ok(AttributeValue::String(value.clone())),
        Json::Array(values) => values
            .iter()
            .map(json_to_attribute_value)
            .collect::<ConfResult<Vec<_>>>()
            .map(AttributeValue::Array),
        Json::Object(values) => values
            .iter()
            .map(|(key, value)| Ok((key.clone(), json_to_attribute_value(value)?)))
            .collect::<ConfResult<BTreeMap<_, _>>>()
            .map(AttributeValue::Object),
    }
}

fn attribute_value_to_json(value: &AttributeValue) -> Json {
    match value {
        AttributeValue::Int(value) => Json::Number(value.to_string()),
        AttributeValue::Bool(value) => Json::Bool(*value),
        AttributeValue::String(value) => Json::String(value.clone()),
        AttributeValue::Object(values) => Json::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), attribute_value_to_json(value)))
                .collect(),
        ),
        AttributeValue::Array(values) => {
            Json::Array(values.iter().map(attribute_value_to_json).collect())
        }
        AttributeValue::Null => Json::Null,
    }
}

fn event_json_value(event: &Event, path: &str) -> Option<Json> {
    let data = event.data.as_ref()?;
    if let Some(current) = data.downcast_ref::<ConformanceEventData>() {
        let payload = current.payload.as_ref()?;
        return json_path(payload, path).cloned();
    }
    if let Some(current) = data.downcast_ref::<Json>() {
        return json_path(current, path).cloned();
    }
    if let Some(call_data) = data.downcast_ref::<CallData>() {
        if path.is_empty() {
            if call_data.Args.len() == 1 {
                return any_to_json(&call_data.Args[0]);
            }
            return Some(call_data_to_json(call_data));
        }
        if call_data.Args.len() == 1 {
            let current = any_to_json(&call_data.Args[0])?;
            return json_path(&current, path).cloned();
        }
        let current = call_data_to_json(call_data);
        return json_path(&current, path).cloned();
    }
    if let Some(change) = data.downcast_ref::<AttributeChange>() {
        if path.is_empty() {
            return Some(attribute_value_to_json(&change.Value));
        }
        let current = Json::object(vec![
            ("name", Json::String(change.Name.clone())),
            (
                "old",
                change
                    .Old
                    .as_ref()
                    .map(attribute_value_to_json)
                    .unwrap_or(Json::Null),
            ),
            ("new", attribute_value_to_json(&change.Value)),
            ("value", attribute_value_to_json(&change.Value)),
        ]);
        return json_path(&current, path).cloned();
    }
    None
}

fn conformance_event_data(event: &Event) -> Option<&ConformanceEventData> {
    event.data.as_ref()?.downcast_ref::<ConformanceEventData>()
}

fn event_metadata_value(event: &Event, name: &str) -> Option<Json> {
    if name == "name" {
        return Some(Json::String(event.name.clone()));
    }
    let data = conformance_event_data(event)?;
    match name {
        "id" => data.id.as_ref().map(|value| Json::String(value.clone())),
        "source" => data
            .source
            .as_ref()
            .map(|value| Json::String(value.clone())),
        "target" => data
            .target
            .as_ref()
            .map(|value| Json::String(value.clone())),
        _ => event_application_metadata_value(event, name),
    }
}

fn event_application_metadata_value(event: &Event, name: &str) -> Option<Json> {
    let data = conformance_event_data(event)?;
    data.metadata.lock().unwrap().get(name).cloned()
}

fn set_event_metadata(event: &Event, name: &str, value: Json) {
    if matches!(name, "name" | "id" | "source" | "target") {
        return;
    }
    if let Some(data) = conformance_event_data(event) {
        data.metadata
            .lock()
            .unwrap()
            .insert(name.to_string(), value);
    }
}

fn event_with_route(event: &Event, source: Option<&str>, target: Option<&str>) -> Event {
    let payload = conformance_event_data(event)
        .map(|data| data.with_route(source, target))
        .unwrap_or_else(|| {
            ConformanceEventData::new(
                event_json_value(event, ""),
                None,
                source.map(str::to_string),
                target.map(str::to_string),
                BTreeMap::new(),
            )
        });
    let mut routed = event.clone();
    routed.data = Some(Arc::new(payload));
    routed
}

fn current_instance_id(ctx: &Context) -> Option<String> {
    let (machine, ok) = FromContext::<ConformanceInstance>(ctx);
    if ok {
        machine.map(|hsm| hsm.id())
    } else {
        None
    }
}

fn any_to_json(value: &Arc<dyn std::any::Any + Send + Sync>) -> Option<Json> {
    value.downcast_ref::<Json>().cloned().or_else(|| {
        value
            .downcast_ref::<AttributeValue>()
            .map(attribute_value_to_json)
    })
}

fn call_data_to_json(call_data: &CallData) -> Json {
    Json::object(vec![
        ("name", Json::String(call_data.Name.clone())),
        (
            "args",
            Json::Array(call_data.Args.iter().filter_map(any_to_json).collect()),
        ),
    ])
}

fn json_path<'a>(root: &'a Json, path: &str) -> Option<&'a Json> {
    if path.is_empty() {
        return Some(root);
    }
    let mut current = root;
    for part in path.split('.') {
        match current {
            Json::Object(object) => {
                current = object.get(part)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

fn json_truthy(value: &Json) -> bool {
    !matches!(value, Json::Null | Json::Bool(false))
}

fn attribute_truthy(value: &AttributeValue) -> bool {
    !matches!(value, AttributeValue::Null | AttributeValue::Bool(false))
}

fn record_context_snapshot(
    ctx: &Context,
    instance: &ConformanceInstance,
    id: &str,
) -> ConfResult<()> {
    let (Some(hsm), true) = FromContext::<ConformanceInstance>(ctx) else {
        return Err(ConfError::Fail(
            "snapshot requires a current HSM context".to_string(),
        ));
    };
    let normalized = normalize_snapshot(&hsm.take_snapshot()?)?;
    let state = required_string(object(&normalized)?, "state")?;
    instance.insert_snapshot(id.to_string(), normalized);
    instance.push_trace(Json::object(vec![
        ("type", Json::String("snapshot".to_string())),
        ("state", Json::String(state)),
    ]));
    Ok(())
}

fn record_snapshot(hsm: &HSM<ConformanceInstance>, id: &str) -> ConfResult<()> {
    let normalized = normalize_snapshot(&hsm.take_snapshot()?)?;
    let state = required_string(object(&normalized)?, "state")?;
    let instance = hsm.instance().read().unwrap();
    instance.insert_snapshot(id.to_string(), normalized);
    instance.push_trace(Json::object(vec![
        ("type", Json::String("snapshot".to_string())),
        ("state", Json::String(state)),
    ]));
    Ok(())
}

fn record_group_snapshot(
    hsm: &HSM<ConformanceInstance>,
    group: &Group<ConformanceInstance>,
    group_id: &str,
    id: &str,
) -> ConfResult<()> {
    let normalized = normalize_group_snapshot(&group.take_snapshot()?)?;
    let instance = hsm.instance().read().unwrap();
    instance.insert_snapshot(id.to_string(), normalized);
    instance.push_trace(Json::object(vec![
        ("type", Json::String("snapshot".to_string())),
        ("group", Json::String(group_id.to_string())),
    ]));
    Ok(())
}

fn normalize_snapshot(snapshot: &Snapshot) -> ConfResult<Json> {
    let mut attributes = BTreeMap::new();
    let prefix = format!("{}/", snapshot.QualifiedName.trim_end_matches('/'));
    for (name, value) in &snapshot.Attributes {
        let normalized_name = name.strip_prefix(&prefix).unwrap_or(name).to_string();
        attributes.insert(normalized_name, attribute_value_to_json(value));
    }

    let transitions = snapshot
        .Transitions
        .iter()
        .filter(|transition| !transition.Events.iter().any(|event| event == "hsm/initial"))
        .map(|transition| {
            Json::object(vec![
                ("name", Json::String(transition.Name.clone())),
                (
                    "kind",
                    Json::Number(normalize_transition_kind(transition.Kind).to_string()),
                ),
                ("source", Json::String(transition.Source.clone())),
                (
                    "target",
                    transition
                        .Target
                        .clone()
                        .map(Json::String)
                        .unwrap_or(Json::Null),
                ),
                (
                    "events",
                    Json::Array(
                        transition
                            .Events
                            .iter()
                            .cloned()
                            .map(Json::String)
                            .collect(),
                    ),
                ),
                ("guard", Json::Bool(transition.Guard)),
            ])
        })
        .collect::<Vec<_>>();

    let mut entries = vec![
        ("id", Json::String(snapshot.ID.clone())),
        (
            "qualified_name",
            Json::String(snapshot.QualifiedName.clone()),
        ),
        ("state", Json::String(snapshot.State.clone())),
        ("attributes", Json::Object(attributes)),
        ("queue_len", Json::Number(snapshot.QueueLen.to_string())),
    ];
    if !transitions.is_empty() {
        entries.push(("transitions", Json::Array(transitions)));
    }

    Ok(Json::object(entries))
}

fn normalize_group_snapshot(snapshots: &[Snapshot]) -> ConfResult<Json> {
    let mut members = BTreeMap::new();
    for snapshot in snapshots {
        members.insert(snapshot.ID.clone(), Json::String(snapshot.State.clone()));
    }
    Ok(Json::object(vec![("members", Json::Object(members))]))
}

fn normalize_transition_kind(kind: kind::KindValue) -> u64 {
    match kind {
        kind::EXTERNAL => 67343,
        kind::SELF => 67344,
        kind::INTERNAL => 67345,
        kind::LOCAL => 67346,
        kind::TRANSITION => 263,
        other => other,
    }
}

fn transition_kind_value(name: &str) -> ConfResult<kind::KindValue> {
    match name {
        "external" => Ok(kind::EXTERNAL),
        "internal" => Ok(kind::INTERNAL),
        "local" => Ok(kind::LOCAL),
        "self" => Ok(kind::SELF),
        other => Err(ConfError::Skip(format!(
            "unsupported transition kind \"{other}\""
        ))),
    }
}

fn actual_attributes(
    hsm: &HSM<ConformanceInstance>,
    expected_attributes: &Json,
) -> ConfResult<Json> {
    let expected = object(expected_attributes)?;
    let mut actual = BTreeMap::new();
    for name in expected.keys() {
        let value = hsm
            .get(name)
            .map(|value| attribute_value_to_json(&value))
            .unwrap_or(Json::Null);
        actual.insert(name.clone(), value);
    }
    Ok(Json::Object(actual))
}

fn assert_partial_json(actual: &Json, expected: &Json, label: &str) -> ConfResult<()> {
    match (actual, expected) {
        (Json::Object(actual), Json::Object(expected)) => {
            for (key, expected_value) in expected {
                let Some(actual_value) = actual.get(key) else {
                    return Err(ConfError::Fail(format!(
                        "{label}.{key} missing: expected {}",
                        format_json(expected_value)
                    )));
                };
                assert_partial_json(actual_value, expected_value, &format!("{label}.{key}"))?;
            }
            Ok(())
        }
        _ if actual == expected => Ok(()),
        _ => Err(ConfError::Fail(format!(
            "{label} mismatch: expected {}, got {}",
            format_json(expected),
            format_json(actual)
        ))),
    }
}

fn expected_trace_contains(expect: &BTreeMap<String, Json>, event_type: &str) -> bool {
    let Some(Json::Array(trace)) = expect.get("trace") else {
        return false;
    };
    trace.iter().any(|entry| {
        object(entry)
            .ok()
            .and_then(|entry| optional_string(entry, "type").ok().flatten())
            .as_deref()
            == Some(event_type)
    })
}

fn parse_case(text: &str) -> ConfResult<CaseData> {
    let json = Parser::new(text).parse()?;
    let root = object(&json)?;
    let name = required_string(root, "name")?;
    let mode = optional_string(root, "mode")?.unwrap_or_else(|| "runtime".to_string());
    let features = optional_array(root, "features")?
        .into_iter()
        .map(string)
        .collect::<ConfResult<Vec<_>>>()?;
    let model = root
        .get("model")
        .cloned()
        .ok_or_else(|| ConfError::Fail("case missing model".to_string()))?;
    let models: Vec<Json> = optional_array(root, "models")?
        .into_iter()
        .cloned()
        .collect();
    let behaviors = parse_behaviors(root.get("behaviors"))?;
    let operation_aliases = parse_operation_aliases(&model, &models)?;
    let combined_guards = parse_combined_guards(&model, &models)?;
    let instances = optional_array(root, "instances")?
        .into_iter()
        .cloned()
        .collect();
    let groups = optional_array(root, "groups")?
        .into_iter()
        .cloned()
        .collect();
    let script = optional_array(root, "script")?
        .into_iter()
        .cloned()
        .collect();
    let expect = match root.get("expect") {
        Some(Json::Object(expect)) => expect.clone(),
        Some(_) => return Err(ConfError::Fail("expect must be an object".to_string())),
        None => BTreeMap::new(),
    };

    Ok(CaseData {
        name,
        features,
        mode,
        model,
        models,
        behaviors,
        operation_aliases,
        combined_guards,
        instances,
        groups,
        script,
        expect,
    })
}

fn parse_behaviors(value: Option<&Json>) -> ConfResult<HashMap<String, Vec<Json>>> {
    let Some(value) = value else {
        return Ok(HashMap::new());
    };
    let Json::Object(object) = value else {
        return Err(ConfError::Fail("behaviors must be an object".to_string()));
    };
    object
        .iter()
        .map(|(key, value)| {
            let Json::Array(program) = value else {
                return Err(ConfError::Fail(format!(
                    "behavior \"{key}\" must be an array"
                )));
            };
            Ok((key.clone(), program.clone()))
        })
        .collect()
}

fn parse_operation_aliases(model: &Json, models: &[Json]) -> ConfResult<HashMap<String, String>> {
    let mut aliases = HashMap::new();
    collect_operation_aliases(model, &mut aliases)?;
    for model in models {
        collect_operation_aliases(model, &mut aliases)?;
    }
    Ok(aliases)
}

fn collect_operation_aliases(
    model: &Json,
    aliases: &mut HashMap<String, String>,
) -> ConfResult<()> {
    let model = object(model)?;
    let Some(operations) = optional_object(model, "operations")? else {
        return Ok(());
    };
    for (name, spec) in operations {
        let spec = object(spec)?;
        aliases.insert(name.clone(), required_string(spec, "behavior")?);
    }
    Ok(())
}

fn parse_combined_guards(
    model: &Json,
    models: &[Json],
) -> ConfResult<HashMap<String, (String, String)>> {
    let mut guards = HashMap::new();
    collect_combined_guards(model, &mut guards)?;
    for model in models {
        collect_combined_guards(model, &mut guards)?;
    }
    Ok(guards)
}

fn collect_combined_guards(
    model: &Json,
    guards: &mut HashMap<String, (String, String)>,
) -> ConfResult<()> {
    let model = object(model)?;
    for transition in optional_array(model, "transitions")? {
        collect_transition_combined_guard(transition, guards)?;
    }
    for state in optional_array(model, "states")? {
        collect_state_combined_guards(state, guards)?;
    }
    Ok(())
}

fn collect_state_combined_guards(
    state: &Json,
    guards: &mut HashMap<String, (String, String)>,
) -> ConfResult<()> {
    let state = object(state)?;
    for transition in optional_array(state, "transitions")? {
        collect_transition_combined_guard(transition, guards)?;
    }
    for child in optional_array(state, "states")? {
        collect_state_combined_guards(child, guards)?;
    }
    Ok(())
}

fn collect_transition_combined_guard(
    transition: &Json,
    guards: &mut HashMap<String, (String, String)>,
) -> ConfResult<()> {
    let transition = object(transition)?;
    let Some(guard) = transition.get("guard") else {
        return Ok(());
    };
    let Some(trigger) = optional_object(transition, "trigger")? else {
        return Ok(());
    };
    if required_string(trigger, "kind")?.as_str() != "when" {
        return Ok(());
    }
    let Some(when_behavior) = optional_string(trigger, "behavior")? else {
        return Ok(());
    };
    let guard_behavior = behavior_ref(guard)?;
    guards.insert(
        combined_guard_name(&when_behavior, &guard_behavior),
        (when_behavior, guard_behavior),
    );
    Ok(())
}

fn model_name(model: &Json) -> ConfResult<String> {
    required_string(object(model)?, "name")
}

fn qualify_operation_from_caller(caller_operation: &str, operation: &str) -> String {
    if operation.starts_with('/') {
        return operation.to_string();
    }

    let Some(root) = caller_operation
        .trim_start_matches('/')
        .split('/')
        .next()
        .filter(|root| !root.is_empty())
    else {
        return operation.to_string();
    };

    format!("/{root}/{operation}")
}

fn qualify_operation_from_context(ctx: &Context, operation: &str) -> String {
    if operation.starts_with('/') {
        return operation.to_string();
    }

    let (Some(hsm), true) = FromContext::<ConformanceInstance>(ctx) else {
        return operation.to_string();
    };
    stateforward_hsm::path::join(&hsm.qualified_name(), operation)
}

fn combined_guard_name(when_behavior: &str, guard_behavior: &str) -> String {
    format!(
        "__when_guard_{}_{}",
        sanitize_operation_name(when_behavior),
        sanitize_operation_name(guard_behavior)
    )
}

fn sanitize_operation_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn unsupported_features(features: &[String]) -> Vec<String> {
    features
        .iter()
        .filter(|feature| !SUPPORTED_FEATURES.contains(&feature.as_str()))
        .cloned()
        .collect()
}

fn has_feature(case: &CaseData, feature: &str) -> bool {
    case.features.iter().any(|item| item == feature)
}

fn behavior_ref(value: &Json) -> ConfResult<String> {
    required_string(object(value)?, "behavior")
}

fn event_name(value: &Json) -> ConfResult<String> {
    match value {
        Json::String(name) => Ok(name.clone()),
        Json::Object(object) => required_string(object, "name"),
        _ => Err(ConfError::Fail(
            "event must be a string or object".to_string(),
        )),
    }
}

fn event_names_from_on_trigger(trigger: &BTreeMap<String, Json>) -> ConfResult<Vec<String>> {
    if let Some(event) = trigger.get("event") {
        return Ok(vec![event_name(event)?]);
    }
    if let Some(Json::Array(events)) = trigger.get("events") {
        return events.iter().map(event_name).collect();
    }
    Err(ConfError::Fail("on trigger missing event".to_string()))
}

fn raise_code(op: &BTreeMap<String, Json>) -> Option<String> {
    match op.get("code") {
        Some(Json::String(code)) => Some(code.clone()),
        _ => None,
    }
}

fn issue_message(value: Option<&Json>) -> String {
    match value {
        Some(Json::String(value)) => value.clone(),
        Some(value) => format_json(value),
        None => String::new(),
    }
}

fn event_from_json(value: &Json) -> ConfResult<Event> {
    match value {
        Json::String(name) => Ok(
            Event::new(name.clone()).with_data(ConformanceEventData::new(
                None,
                None,
                None,
                None,
                BTreeMap::new(),
            )),
        ),
        Json::Object(object) => {
            let metadata = optional_object(object, "metadata")?
                .cloned()
                .unwrap_or_default();
            Ok(
                Event::new(required_string(object, "name")?).with_data(ConformanceEventData::new(
                    object.get("data").cloned(),
                    optional_string(object, "id")?,
                    optional_string(object, "source")?,
                    optional_string(object, "target")?,
                    metadata,
                )),
            )
        }
        _ => Err(ConfError::Fail(
            "event must be a string or object".to_string(),
        )),
    }
}

fn call_args_from_step(
    step: &BTreeMap<String, Json>,
) -> ConfResult<Vec<Arc<dyn std::any::Any + Send + Sync>>> {
    Ok(match step.get("data") {
        Some(data) => vec![Arc::new(data.clone()) as Arc<dyn std::any::Any + Send + Sync>],
        None => Vec::new(),
    })
}

fn resolve_initial_path(owner_path: &str, path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        normalize_path(owner_path, path)
    }
}

fn resolve_transition_path(model_root: &str, owner_path: &str, path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else if path.starts_with("./") || path.starts_with("../") {
        normalize_path(owner_path, path)
    } else {
        normalize_path(model_root, path)
    }
}

fn resolve_transition_target_path(
    model_root: &str,
    owner_path: &str,
    source_path: Option<&str>,
    target_path: &str,
) -> String {
    if target_path == "." {
        source_path
            .map(str::to_string)
            .unwrap_or_else(|| normalize_path(owner_path, "."))
    } else {
        resolve_transition_path(model_root, owner_path, target_path)
    }
}

fn normalize_path(base: &str, path: &str) -> String {
    let mut parts: Vec<&str> = base.split('/').filter(|part| !part.is_empty()).collect();
    for part in path.split('/').filter(|part| !part.is_empty()) {
        match part {
            "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/{}", parts.join("/"))
}

fn join_path(owner_path: &str, name: &str) -> String {
    format!("{}/{}", owner_path.trim_end_matches('/'), name)
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn object(value: &Json) -> ConfResult<&BTreeMap<String, Json>> {
    match value {
        Json::Object(object) => Ok(object),
        _ => Err(ConfError::Fail("expected object".to_string())),
    }
}

fn string(value: &Json) -> ConfResult<String> {
    match value {
        Json::String(value) => Ok(value.clone()),
        _ => Err(ConfError::Fail("expected string".to_string())),
    }
}

fn required_string(object: &BTreeMap<String, Json>, key: &str) -> ConfResult<String> {
    let value = object
        .get(key)
        .ok_or_else(|| ConfError::Fail(format!("missing string field \"{key}\"")))?;
    string(value)
}

fn optional_string(object: &BTreeMap<String, Json>, key: &str) -> ConfResult<Option<String>> {
    object.get(key).map(string).transpose()
}

fn optional_number_u64(object: &BTreeMap<String, Json>, key: &str) -> ConfResult<Option<u64>> {
    object
        .get(key)
        .map(|value| json_u64(value, key))
        .transpose()
}

fn optional_object<'a>(
    object: &'a BTreeMap<String, Json>,
    key: &str,
) -> ConfResult<Option<&'a BTreeMap<String, Json>>> {
    match object.get(key) {
        Some(Json::Object(value)) => Ok(Some(value)),
        Some(_) => Err(ConfError::Fail(format!(
            "field \"{key}\" must be an object"
        ))),
        None => Ok(None),
    }
}

fn optional_array<'a>(object: &'a BTreeMap<String, Json>, key: &str) -> ConfResult<Vec<&'a Json>> {
    match object.get(key) {
        Some(Json::Array(values)) => Ok(values.iter().collect()),
        Some(_) => Err(ConfError::Fail(format!("field \"{key}\" must be an array"))),
        None => Ok(Vec::new()),
    }
}

fn collect_case_files(roots: &[PathBuf]) -> ConfResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    for root in roots {
        collect_case_files_from(root, &mut files)?;
    }
    files.sort();
    Ok(files)
}

fn collect_case_files_from(root: &Path, files: &mut Vec<PathBuf>) -> ConfResult<()> {
    if root.is_file() {
        if root.extension().and_then(|ext| ext.to_str()) == Some("json") {
            files.push(root.to_path_buf());
        }
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        collect_case_files_from(&entry?.path(), files)?;
    }
    Ok(())
}

fn format_json(value: &Json) -> String {
    match value {
        Json::Null => "null".to_string(),
        Json::Bool(value) => value.to_string(),
        Json::Number(value) => value.clone(),
        Json::String(value) => format!("{value:?}"),
        Json::Array(values) => format!(
            "[{}]",
            values.iter().map(format_json).collect::<Vec<_>>().join(",")
        ),
        Json::Object(object) => format!(
            "{{{}}}",
            object
                .iter()
                .map(|(key, value)| format!("{key:?}:{}", format_json(value)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    fn parse(mut self) -> ConfResult<Json> {
        let value = self.parse_value()?;
        self.skip_ws();
        if self.pos != self.input.len() {
            return Err(ConfError::Fail(
                "unexpected trailing JSON input".to_string(),
            ));
        }
        Ok(value)
    }

    fn parse_value(&mut self) -> ConfResult<Json> {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => self.parse_literal(b"null", Json::Null),
            Some(b't') => self.parse_literal(b"true", Json::Bool(true)),
            Some(b'f') => self.parse_literal(b"false", Json::Bool(false)),
            Some(b'"') => self.parse_string().map(Json::String),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-' | b'0'..=b'9') => self.parse_number().map(Json::Number),
            Some(_) => Err(ConfError::Fail("unexpected JSON token".to_string())),
            None => Err(ConfError::Fail("unexpected end of JSON input".to_string())),
        }
    }

    fn parse_literal(&mut self, literal: &[u8], value: Json) -> ConfResult<Json> {
        if self.input.get(self.pos..self.pos + literal.len()) == Some(literal) {
            self.pos += literal.len();
            Ok(value)
        } else {
            Err(ConfError::Fail("invalid JSON literal".to_string()))
        }
    }

    fn parse_array(&mut self) -> ConfResult<Json> {
        self.expect(b'[')?;
        let mut values = Vec::new();
        loop {
            self.skip_ws();
            if self.consume(b']') {
                break;
            }
            values.push(self.parse_value()?);
            self.skip_ws();
            if self.consume(b']') {
                break;
            }
            self.expect(b',')?;
        }
        Ok(Json::Array(values))
    }

    fn parse_object(&mut self) -> ConfResult<Json> {
        self.expect(b'{')?;
        let mut object = BTreeMap::new();
        loop {
            self.skip_ws();
            if self.consume(b'}') {
                break;
            }
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            object.insert(key, value);
            self.skip_ws();
            if self.consume(b'}') {
                break;
            }
            self.expect(b',')?;
        }
        Ok(Json::Object(object))
    }

    fn parse_string(&mut self) -> ConfResult<String> {
        self.expect(b'"')?;
        let mut output = String::new();
        while let Some(byte) = self.next() {
            match byte {
                b'"' => return Ok(output),
                b'\\' => output.push(self.parse_escape()?),
                0x00..=0x1f => {
                    return Err(ConfError::Fail(
                        "control character in JSON string".to_string(),
                    ));
                }
                _ => {
                    let start = self.pos - 1;
                    while let Some(next) = self.peek() {
                        if next == b'"' || next == b'\\' || next <= 0x1f {
                            break;
                        }
                        self.pos += 1;
                    }
                    let slice = std::str::from_utf8(&self.input[start..self.pos])
                        .map_err(|error| ConfError::Fail(error.to_string()))?;
                    output.push_str(slice);
                }
            }
        }
        Err(ConfError::Fail("unterminated JSON string".to_string()))
    }

    fn parse_escape(&mut self) -> ConfResult<char> {
        match self.next() {
            Some(b'"') => Ok('"'),
            Some(b'\\') => Ok('\\'),
            Some(b'/') => Ok('/'),
            Some(b'b') => Ok('\u{0008}'),
            Some(b'f') => Ok('\u{000c}'),
            Some(b'n') => Ok('\n'),
            Some(b'r') => Ok('\r'),
            Some(b't') => Ok('\t'),
            Some(b'u') => self.parse_unicode_escape(),
            _ => Err(ConfError::Fail("invalid JSON escape".to_string())),
        }
    }

    fn parse_unicode_escape(&mut self) -> ConfResult<char> {
        let mut value = 0u32;
        for _ in 0..4 {
            let digit = self
                .next()
                .ok_or_else(|| ConfError::Fail("short JSON unicode escape".to_string()))?;
            value = value * 16
                + match digit {
                    b'0'..=b'9' => (digit - b'0') as u32,
                    b'a'..=b'f' => (digit - b'a' + 10) as u32,
                    b'A'..=b'F' => (digit - b'A' + 10) as u32,
                    _ => {
                        return Err(ConfError::Fail("invalid JSON unicode escape".to_string()));
                    }
                };
        }
        char::from_u32(value).ok_or_else(|| ConfError::Fail("invalid unicode scalar".to_string()))
    }

    fn parse_number(&mut self) -> ConfResult<String> {
        let start = self.pos;
        self.consume(b'-');
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
            }
            Some(b'1'..=b'9') => {
                self.pos += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err(ConfError::Fail("invalid JSON number".to_string())),
        }
        if self.consume(b'.') {
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(ConfError::Fail("invalid JSON number".to_string()));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(ConfError::Fail("invalid JSON number".to_string()));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        std::str::from_utf8(&self.input[start..self.pos])
            .map(|value| value.to_string())
            .map_err(|error| ConfError::Fail(error.to_string()))
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, expected: u8) -> ConfResult<()> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(ConfError::Fail(format!(
                "expected JSON byte {:?}",
                expected as char
            )))
        }
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.pos += 1;
        Some(byte)
    }
}
