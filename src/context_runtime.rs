use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};

use crate::context::Context;
use crate::element::{AttributeValue, Instance};
use crate::error::{HsmError, Result};
use crate::event::Event;
use crate::hsm_impl::HSM;

pub trait ContextMachine: Any + Send + Sync {
    fn id(&self) -> String;
    fn qualified_name(&self) -> String;
    fn state(&self) -> String;
    fn is_started(&self) -> bool;
    fn can_receive_dispatch(&self) -> bool {
        self.is_started()
    }
    fn dispatch_event(
        &self,
        ctx: &Context,
        event: Event,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
    fn get_attribute_value(&self, _name: &str) -> Option<AttributeValue> {
        None
    }
    fn set_attribute_value(&self, _name: &str, _value: AttributeValue) -> Result<()> {
        Err(HsmError::Runtime("set requires a started HSM".to_string()))
    }
    fn set_attribute_value_with_context(
        &self,
        _ctx: &Context,
        name: &str,
        value: AttributeValue,
    ) -> Result<()> {
        self.set_attribute_value(name, value)
    }
    fn call_operation_value(
        &self,
        _ctx: &Context,
        _name: &str,
        _args: Vec<Arc<dyn Any + Send + Sync>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        Box::pin(async {
            Err(HsmError::Runtime(
                "operation requires a started HSM".to_string(),
            ))
        })
    }
    fn as_any(&self) -> &dyn Any;
}

struct ContextRegistry {
    current: Option<String>,
    machines: HashMap<String, Arc<dyn ContextMachine>>,
}

static CONTEXT_REGISTRIES: OnceLock<Mutex<HashMap<usize, ContextRegistry>>> = OnceLock::new();

fn context_registries() -> &'static Mutex<HashMap<usize, ContextRegistry>> {
    CONTEXT_REGISTRIES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn context_machine_id(machine_id: String, machine_name: String) -> String {
    if machine_id.is_empty() {
        machine_name
    } else {
        machine_id
    }
}

pub(crate) fn register_context_machine<T: Instance>(ctx: &Context, machine: &HSM<T>) {
    let id = context_machine_id(machine.id(), machine.qualified_name());
    let handle: Arc<dyn ContextMachine> = Arc::new(machine.clone());
    let mut registries = context_registries().lock().unwrap();
    let registry = registries
        .entry(ctx.registry_key())
        .or_insert_with(|| ContextRegistry {
            current: None,
            machines: HashMap::new(),
        });
    registry.current = Some(id.clone());
    registry.machines.insert(id, handle);
}

pub(crate) fn unregister_context_machine<T: Instance>(ctx: &Context, machine: &HSM<T>) {
    let id = context_machine_id(machine.id(), machine.qualified_name());
    let mut registries = context_registries().lock().unwrap();
    let Some(registry) = registries.get_mut(&ctx.registry_key()) else {
        return;
    };

    registry.machines.remove(&id);
    if registry.current.as_deref() == Some(id.as_str()) {
        registry.current = None;
    }
    if registry.machines.is_empty() {
        registries.remove(&ctx.registry_key());
    }
}

fn current_context_machine(ctx: &Context) -> Option<Arc<dyn ContextMachine>> {
    let registries = context_registries().lock().unwrap();
    let registry = registries.get(&ctx.registry_key())?;
    let current = registry.current.as_ref()?;
    registry.machines.get(current).cloned()
}

fn context_machines(ctx: &Context) -> (Vec<Arc<dyn ContextMachine>>, bool) {
    let registries = context_registries().lock().unwrap();
    let Some(registry) = registries.get(&ctx.registry_key()) else {
        return (Vec::new(), false);
    };

    let mut machines: Vec<_> = registry.machines.values().cloned().collect();
    machines.sort_by_key(|machine| machine.id());
    (machines, true)
}

pub fn from_context<T: Instance + 'static>(ctx: &Context) -> (Option<HSM<T>>, bool) {
    let Some(machine) = current_context_machine(ctx) else {
        return (None, false);
    };
    let Some(typed) = machine.as_any().downcast_ref::<HSM<T>>() else {
        return (None, false);
    };
    (Some(typed.clone()), true)
}

#[allow(non_snake_case)]
pub fn FromContext<T: Instance + 'static>(ctx: &Context) -> (Option<HSM<T>>, bool) {
    from_context(ctx)
}

pub fn dispatch_from_context(
    ctx: &Context,
    event: Event,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    let Some(machine) = current_context_machine(ctx) else {
        return Box::pin(async {
            Err(HsmError::Runtime(
                "dispatch requires a started HSM".to_string(),
            ))
        });
    };
    machine.dispatch_event(ctx, event)
}

#[allow(non_snake_case)]
pub fn DispatchFromContext(
    ctx: &Context,
    event: Event,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    dispatch_from_context(ctx, event)
}

pub fn get_from_context(ctx: &Context, name: &str) -> Option<AttributeValue> {
    current_context_machine(ctx).and_then(|machine| machine.get_attribute_value(name))
}

#[allow(non_snake_case)]
pub fn GetFromContext(ctx: &Context, name: &str) -> Option<AttributeValue> {
    get_from_context(ctx, name)
}

pub fn set_from_context<V: Into<AttributeValue>>(
    ctx: &Context,
    name: &str,
    value: V,
) -> Result<()> {
    let Some(machine) = current_context_machine(ctx) else {
        return Err(HsmError::Runtime("set requires a started HSM".to_string()));
    };
    machine.set_attribute_value_with_context(ctx, name, value.into())
}

#[allow(non_snake_case)]
pub fn SetFromContext<V: Into<AttributeValue>>(ctx: &Context, name: &str, value: V) -> Result<()> {
    set_from_context(ctx, name, value)
}

pub fn call_from_context(
    ctx: &Context,
    name: &str,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    call_with_args_from_context(ctx, name, Vec::new())
}

pub fn call_with_args_from_context(
    ctx: &Context,
    name: &str,
    args: Vec<Arc<dyn Any + Send + Sync>>,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    let Some(machine) = current_context_machine(ctx) else {
        return Box::pin(async {
            Err(HsmError::Runtime(
                "operation requires a started HSM".to_string(),
            ))
        });
    };
    machine.call_operation_value(ctx, name, args)
}

#[allow(non_snake_case)]
pub fn CallFromContext(
    ctx: &Context,
    name: &str,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    call_from_context(ctx, name)
}

#[allow(non_snake_case)]
pub fn CallWithArgsFromContext(
    ctx: &Context,
    name: &str,
    args: Vec<Arc<dyn Any + Send + Sync>>,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    call_with_args_from_context(ctx, name, args)
}

pub fn instances_from_context(ctx: &Context) -> (Vec<Arc<dyn ContextMachine>>, bool) {
    let (machines, ok) = context_machines(ctx);
    let machines = machines
        .into_iter()
        .filter(|machine| machine.is_started())
        .collect();
    (machines, ok)
}

#[allow(non_snake_case)]
pub fn InstancesFromContext(ctx: &Context) -> (Vec<Arc<dyn ContextMachine>>, bool) {
    instances_from_context(ctx)
}

pub fn dispatch_all(
    ctx: &Context,
    event: Event,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    dispatch_to(ctx, event, Vec::<String>::new())
}

#[allow(non_snake_case)]
pub fn DispatchAll(
    ctx: &Context,
    event: Event,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    dispatch_all(ctx, event)
}

pub fn dispatch_to<S>(
    ctx: &Context,
    event: Event,
    ids: Vec<S>,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
where
    S: AsRef<str> + Send + 'static,
{
    let ctx = ctx.clone();
    let patterns: Vec<String> = ids.into_iter().map(|id| id.as_ref().to_string()).collect();
    Box::pin(async move {
        let (machines, _) = context_machines(&ctx);
        let machines: Vec<_> = machines
            .into_iter()
            .filter(|machine| machine.can_receive_dispatch())
            .filter(|machine| {
                patterns.is_empty()
                    || patterns
                        .iter()
                        .any(|pattern| dispatch_id_matches(pattern, &machine.id()))
            })
            .collect();

        for machine in machines {
            machine.dispatch_event(&ctx, event.clone()).await?;
        }
        Ok(())
    })
}

#[allow(non_snake_case)]
pub fn DispatchTo<S>(
    ctx: &Context,
    event: Event,
    ids: Vec<S>,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
where
    S: AsRef<str> + Send + 'static,
{
    dispatch_to(ctx, event, ids)
}

fn dispatch_id_matches(pattern: &str, id: &str) -> bool {
    if pattern == id {
        return true;
    }
    if !pattern.contains('*') && !pattern.contains('?') {
        return false;
    }

    dispatch_pattern_matches(pattern.as_bytes(), id.as_bytes())
}

fn dispatch_pattern_matches(pattern: &[u8], text: &[u8]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }

    match pattern[0] {
        b'*' => {
            dispatch_pattern_matches(&pattern[1..], text)
                || (!text.is_empty() && dispatch_pattern_matches(pattern, &text[1..]))
        }
        b'?' => !text.is_empty() && dispatch_pattern_matches(&pattern[1..], &text[1..]),
        byte => {
            !text.is_empty()
                && byte == text[0]
                && dispatch_pattern_matches(&pattern[1..], &text[1..])
        }
    }
}
