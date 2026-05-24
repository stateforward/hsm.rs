// Macros for isomorphic API (exact syntax: hsm::define!, hsm::state!, etc.)

#[macro_export]
macro_rules! define {
    ($name:expr, $($element:expr),* $(,)?) => {
        {
            let elements: Vec<Box<dyn $crate::builder::PartialElement<_>>> = vec![$($element),*];
            $crate::define($name, elements)
        }
    };
}

#[macro_export]
macro_rules! state {
    ($name:expr) => {
        $crate::builder::state($name)
    };
    ($name:expr, $($element:expr),* $(,)?) => {
        {
            let mut state = $crate::builder::PartialState {
                name: $name.to_string(),
                elements: vec![$($element),*],
            };
            Box::new(state) as Box<dyn $crate::builder::PartialElement<_>>
        }
    };
}

#[macro_export]
macro_rules! transition {
    ($($element:expr),* $(,)?) => {
        {
            let mut transition = $crate::builder::PartialTransition {
                name: String::new(),
                elements: vec![$($element),*],
            };
            Box::new(transition) as Box<dyn $crate::builder::PartialElement<_>>
        }
    };
}

#[macro_export]
macro_rules! initial {
    ($($element:expr),* $(,)?) => {
        {
            let mut initial = $crate::builder::PartialInitial {
                name: ".initial".to_string(),
                elements: vec![$($element),*],
            };
            Box::new(initial) as Box<dyn $crate::builder::PartialElement<_>>
        }
    };
}

#[macro_export]
macro_rules! source {
    ($name:expr) => {{
        let source = $crate::builder::PartialSource {
            source: $name.to_string(),
        };
        Box::new(source) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! target {
    ($name:expr) => {{
        let target = $crate::builder::PartialTarget {
            target: $name.to_string(),
        };
        Box::new(target) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! on {
    ($event:expr) => {
        {
            let trigger = $crate::builder::PartialTrigger {
                events: vec![$event.to_string()],
            };
            Box::new(trigger) as Box<dyn $crate::builder::PartialElement<_>>
        }
    };
    ($($event:expr),* $(,)?) => {
        {
            let trigger = $crate::builder::PartialTrigger {
                events: vec![$($event.to_string()),*],
            };
            Box::new(trigger) as Box<dyn $crate::builder::PartialElement<_>>
        }
    };
}

#[macro_export]
macro_rules! on_call {
    ($name:expr) => {{
        let trigger = $crate::builder::PartialOnCall {
            name: $name.to_string(),
        };
        Box::new(trigger) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! on_set {
    ($name:expr) => {{
        let trigger = $crate::builder::PartialOnSet {
            name: $name.to_string(),
        };
        Box::new(trigger) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! when {
    ($name:expr) => {
        $crate::on_set!($name)
    };
}

#[macro_export]
macro_rules! effect {
    ($operation:expr) => {{
        let effect = $crate::builder::PartialEffect {
            operations: vec![$operation],
        };
        Box::new(effect) as Box<dyn $crate::builder::PartialElement<_>>
    }};
    ($($operation:expr),* $(,)?) => {{
        let effect = $crate::builder::PartialEffect {
            operations: vec![$($operation),*],
        };
        Box::new(effect) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! operation {
    ($name:expr, $operation:expr) => {{
        let operation = $crate::builder::PartialOperation {
            name: $name.to_string(),
            action: $operation,
        };
        Box::new(operation) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! guard_operation {
    ($name:expr, $operation:expr) => {{
        let operation = $crate::builder::PartialGuardOperation {
            name: $name.to_string(),
            guard: $operation,
        };
        Box::new(operation) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! entry_operation {
    ($name:expr) => {{
        let operation = $crate::builder::PartialBehaviorOperation {
            name: $name.to_string(),
            role: $crate::builder::BehaviorRole::Entry,
        };
        Box::new(operation) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! exit_operation {
    ($name:expr) => {{
        let operation = $crate::builder::PartialBehaviorOperation {
            name: $name.to_string(),
            role: $crate::builder::BehaviorRole::Exit,
        };
        Box::new(operation) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! activity_operation {
    ($name:expr) => {{
        let operation = $crate::builder::PartialBehaviorOperation {
            name: $name.to_string(),
            role: $crate::builder::BehaviorRole::Activity,
        };
        Box::new(operation) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! effect_operation {
    ($name:expr) => {{
        let operation = $crate::builder::PartialBehaviorOperation {
            name: $name.to_string(),
            role: $crate::builder::BehaviorRole::Effect,
        };
        Box::new(operation) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! entry {
    ($operation:expr) => {{
        let entry = $crate::builder::PartialEntry {
            operations: vec![$operation],
        };
        Box::new(entry) as Box<dyn $crate::builder::PartialElement<_>>
    }};
    ($($operation:expr),* $(,)?) => {{
        let entry = $crate::builder::PartialEntry {
            operations: vec![$($operation),*],
        };
        Box::new(entry) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! exit {
    ($operation:expr) => {{
        let exit = $crate::builder::PartialExit {
            operations: vec![$operation],
        };
        Box::new(exit) as Box<dyn $crate::builder::PartialElement<_>>
    }};
    ($($operation:expr),* $(,)?) => {{
        let exit = $crate::builder::PartialExit {
            operations: vec![$($operation),*],
        };
        Box::new(exit) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! guard {
    ($expression:expr) => {{
        let guard = $crate::builder::PartialGuard {
            expression: $expression,
        };
        Box::new(guard) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! guard_operation_ref {
    ($name:expr) => {{
        let guard = $crate::builder::PartialGuardOperationRef {
            name: $name.to_string(),
        };
        Box::new(guard) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! activity {
    ($operation:expr) => {{
        let activity = $crate::builder::PartialActivity {
            operations: vec![$operation],
        };
        Box::new(activity) as Box<dyn $crate::builder::PartialElement<_>>
    }};
    ($($operation:expr),* $(,)?) => {{
        let activity = $crate::builder::PartialActivity {
            operations: vec![$($operation),*],
        };
        Box::new(activity) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! choice {
    ($name:expr, $($element:expr),* $(,)?) => {
        {
            let mut choice = $crate::builder::PartialChoice {
                name: $name.to_string(),
                elements: vec![$($element),*],
            };
            Box::new(choice) as Box<dyn $crate::builder::PartialElement<_>>
        }
    };
}

#[macro_export]
macro_rules! shallow_history {
    ($name:expr $(,)?) => {
        {
            let history = $crate::builder::PartialHistory {
                name: $name.to_string(),
                kind: $crate::kind::SHALLOW_HISTORY,
                elements: vec![],
            };
            Box::new(history) as Box<dyn $crate::builder::PartialElement<_>>
        }
    };
    ($name:expr, $($element:expr),* $(,)?) => {
        {
            let history = $crate::builder::PartialHistory {
                name: $name.to_string(),
                kind: $crate::kind::SHALLOW_HISTORY,
                elements: vec![$($element),*],
            };
            Box::new(history) as Box<dyn $crate::builder::PartialElement<_>>
        }
    };
}

#[macro_export]
macro_rules! deep_history {
    ($name:expr $(,)?) => {
        {
            let history = $crate::builder::PartialHistory {
                name: $name.to_string(),
                kind: $crate::kind::DEEP_HISTORY,
                elements: vec![],
            };
            Box::new(history) as Box<dyn $crate::builder::PartialElement<_>>
        }
    };
    ($name:expr, $($element:expr),* $(,)?) => {
        {
            let history = $crate::builder::PartialHistory {
                name: $name.to_string(),
                kind: $crate::kind::DEEP_HISTORY,
                elements: vec![$($element),*],
            };
            Box::new(history) as Box<dyn $crate::builder::PartialElement<_>>
        }
    };
}

#[macro_export]
macro_rules! ShallowHistory {
    ($($tt:tt)*) => {
        $crate::shallow_history!($($tt)*)
    };
}

#[macro_export]
macro_rules! DeepHistory {
    ($($tt:tt)*) => {
        $crate::deep_history!($($tt)*)
    };
}

#[macro_export]
macro_rules! final_state {
    ($name:expr) => {{
        let final_state = $crate::builder::PartialFinalState {
            name: $name.to_string(),
        };
        Box::new(final_state) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! r#final {
    ($name:expr) => {{
        let final_state = $crate::builder::PartialFinalState {
            name: $name.to_string(),
        };
        Box::new(final_state) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! defer {
    ($($event:expr),* $(,)?) => {
        {
            let defer = $crate::builder::PartialDefer {
                events: vec![$($event.to_string()),*],
            };
            Box::new(defer) as Box<dyn $crate::builder::PartialElement<_>>
        }
    };
}

#[macro_export]
macro_rules! after {
    ($duration_fn:expr) => {{
        let after = $crate::builder::PartialAfter {
            duration_fn: $duration_fn,
        };
        Box::new(after) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! at {
    ($timepoint_fn:expr) => {{
        let at = $crate::builder::PartialAt {
            timepoint_fn: $timepoint_fn,
        };
        Box::new(at) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}

#[macro_export]
macro_rules! every {
    ($duration_fn:expr) => {{
        let every = $crate::builder::PartialEvery {
            duration_fn: $duration_fn,
        };
        Box::new(every) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}
