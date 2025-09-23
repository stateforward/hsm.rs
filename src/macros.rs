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
macro_rules! every {
    ($duration_fn:expr) => {{
        let every = $crate::builder::PartialEvery {
            duration_fn: $duration_fn,
        };
        Box::new(every) as Box<dyn $crate::builder::PartialElement<_>>
    }};
}
