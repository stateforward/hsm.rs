// Event types

use crate::kind::{self, KindValue};
use std::any::Any;
use std::sync::Arc;

#[derive(Debug)]
pub struct Event {
    pub kind: KindValue,
    pub qualified_name: String,
    pub name: String,
    pub data: Option<Arc<dyn Any + Send + Sync>>,
}

impl Clone for Event {
    fn clone(&self) -> Self {
        // Share the Arc data when cloning (as expected by tests)
        Self {
            kind: self.kind,
            qualified_name: self.qualified_name.clone(),
            name: self.name.clone(),
            data: self.data.clone(),
        }
    }
}

impl Event {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            kind: kind::EVENT,
            qualified_name: name.clone(),
            name,
            data: None,
        }
    }

    pub fn with_data<T: Any + Send + Sync>(self, value: T) -> Self {
        Self {
            kind: self.kind,
            qualified_name: self.qualified_name,
            name: self.name,
            data: Some(Arc::new(value)),
        }
    }

    pub fn get_data<T: Any>(&self) -> Option<&T> {
        self.data.as_ref()?.downcast_ref::<T>()
    }

    pub fn completion(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            kind: kind::COMPLETION_EVENT,
            qualified_name: name.clone(),
            name,
            data: None,
        }
    }

    pub fn time_event(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            kind: kind::TIME_EVENT,
            qualified_name: name.clone(),
            name,
            data: None,
        }
    }

    pub fn call(name: impl Into<String>) -> Self {
        let name = name.into();
        let event_name = call_event_name(&name);
        Self {
            kind: kind::CALL_EVENT,
            qualified_name: name,
            name: event_name,
            data: None,
        }
    }

    pub fn error_event() -> Self {
        Self {
            kind: kind::ERROR_EVENT,
            qualified_name: "hsm_error".to_string(),
            name: "hsm_error".to_string(),
            data: None,
        }
    }
}

pub fn call_event_name(name: &str) -> String {
    format!("hsm_call:{name}")
}

// Standard events
pub fn initial_event() -> Event {
    Event::completion("hsm_initial")
}

pub fn final_event() -> Event {
    Event::completion("hsm_final")
}
