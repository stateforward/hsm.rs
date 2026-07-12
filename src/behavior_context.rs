use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::Context;

thread_local! {
    static OPERATION_NAMES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static ABORT_REQUESTED: RefCell<bool> = const { RefCell::new(false) };
    static TIMER_REGISTRATIONS: RefCell<Vec<TimerRegistration>> = const { RefCell::new(Vec::new()) };
}

static RUNNING_ACTIVITIES: OnceLock<Mutex<HashMap<usize, String>>> = OnceLock::new();

fn running_activities() -> &'static Mutex<HashMap<usize, String>> {
    RUNNING_ACTIVITIES.get_or_init(|| Mutex::new(HashMap::new()))
}

#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct TimerRegistration {
    pub instance_id: String,
    pub source_state: String,
    pub event_name: String,
    pub context: Context,
    pub context_key: usize,
}

#[doc(hidden)]
pub fn current_operation_name() -> Option<String> {
    OPERATION_NAMES.with(|names| names.borrow().last().cloned())
}

pub(crate) fn with_operation_name<R>(name: &str, run: impl FnOnce() -> R) -> R {
    OPERATION_NAMES.with(|names| names.borrow_mut().push(name.to_string()));
    let result = run();
    OPERATION_NAMES.with(|names| {
        names.borrow_mut().pop();
    });
    result
}

#[doc(hidden)]
pub fn request_abort() {
    ABORT_REQUESTED.with(|abort| {
        *abort.borrow_mut() = true;
    });
}

pub(crate) fn take_abort() -> bool {
    ABORT_REQUESTED.with(|abort| {
        let requested = *abort.borrow();
        *abort.borrow_mut() = false;
        requested
    })
}

#[doc(hidden)]
pub fn begin_activity(ctx: &Context, behavior: &str) {
    running_activities()
        .lock()
        .unwrap()
        .insert(ctx.registry_key(), behavior.to_string());
}

#[doc(hidden)]
pub fn end_activity(ctx: &Context) {
    running_activities()
        .lock()
        .unwrap()
        .remove(&ctx.registry_key());
}

#[doc(hidden)]
pub fn current_activity(ctx: &Context) -> Option<String> {
    running_activities()
        .lock()
        .unwrap()
        .get(&ctx.registry_key())
        .cloned()
}

#[doc(hidden)]
pub fn current_timer_registration() -> Option<TimerRegistration> {
    TIMER_REGISTRATIONS.with(|registrations| registrations.borrow().last().cloned())
}

#[doc(hidden)]
pub fn with_timer_registration<R>(
    instance_id: &str,
    source_state: &str,
    event_name: &str,
    context: &Context,
    run: impl FnOnce() -> R,
) -> R {
    TIMER_REGISTRATIONS.with(|registrations| {
        registrations.borrow_mut().push(TimerRegistration {
            instance_id: instance_id.to_string(),
            source_state: source_state.to_string(),
            event_name: event_name.to_string(),
            context_key: context.registry_key(),
            context: context.clone(),
        });
    });
    let result = run();
    TIMER_REGISTRATIONS.with(|registrations| {
        registrations.borrow_mut().pop();
    });
    result
}
