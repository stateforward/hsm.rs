// Event types

use crate::element::AttributeValue;
use crate::kind::{self, KindValue};
use std::any::Any;
use std::sync::Arc;
use std::time::SystemTime;

pub const ANY_EVENT_NAME: &str = "*";
pub const OBSERVATION_EVENT_NAME: &str = "hsm/observation";

#[allow(non_upper_case_globals)]
pub const AnyEvent: &str = ANY_EVENT_NAME;

#[allow(non_snake_case)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeChange {
    pub Name: String,
    pub Old: Option<AttributeValue>,
    pub Value: AttributeValue,
}

#[allow(non_snake_case)]
#[derive(Clone)]
pub struct CallData {
    pub Name: String,
    pub Args: Vec<Arc<dyn Any + Send + Sync>>,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone)]
pub struct ObservationData {
    pub Event: Event,
    pub Occurrence: String,
    pub Time: SystemTime,
}

pub trait IntoEventName {
    fn into_event_name(self) -> String;
}

impl IntoEventName for &str {
    fn into_event_name(self) -> String {
        self.to_string()
    }
}

impl IntoEventName for String {
    fn into_event_name(self) -> String {
        self
    }
}

impl IntoEventName for &String {
    fn into_event_name(self) -> String {
        self.clone()
    }
}

impl IntoEventName for Event {
    fn into_event_name(self) -> String {
        self.name
    }
}

impl IntoEventName for &Event {
    fn into_event_name(self) -> String {
        self.name.clone()
    }
}

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
        Self::call_with_args(name, Vec::new())
    }

    pub fn call_with_args(name: impl Into<String>, args: Vec<Arc<dyn Any + Send + Sync>>) -> Self {
        let name = name.into();
        let event_name = call_event_name(&name);
        Self {
            kind: kind::CALL_EVENT,
            qualified_name: name.clone(),
            name: event_name,
            data: Some(Arc::new(CallData {
                Name: name,
                Args: args,
            })),
        }
    }

    pub fn error_event() -> Self {
        Self {
            kind: kind::ERROR_EVENT,
            qualified_name: "hsm/error".to_string(),
            name: "hsm/error".to_string(),
            data: None,
        }
    }

    pub fn observation(
        _source: impl Into<String>,
        occurrence: impl Into<String>,
        observed: Event,
    ) -> Self {
        Self {
            kind: kind::EVENT,
            qualified_name: OBSERVATION_EVENT_NAME.to_string(),
            name: OBSERVATION_EVENT_NAME.to_string(),
            data: Some(Arc::new(ObservationData {
                Event: observed,
                Occurrence: occurrence.into(),
                Time: SystemTime::now(),
            })),
        }
    }
}

pub fn call_event_name(name: &str) -> String {
    name.to_string()
}

pub fn call_trigger_name(name: &str) -> String {
    format!("hsm_call:{}", name)
}

// Standard events
pub fn initial_event() -> Event {
    Event::completion("hsm/initial")
}

pub fn final_event() -> Event {
    Event::completion("hsm/final")
}

pub fn any_event() -> Event {
    Event::new(ANY_EVENT_NAME)
}

pub fn observation_event(
    source: impl Into<String>,
    occurrence: impl Into<String>,
    observed: Event,
) -> Event {
    Event::observation(source, occurrence, observed)
}

#[allow(non_snake_case)]
pub fn InitialEvent() -> Event {
    initial_event()
}

#[allow(non_snake_case)]
pub fn FinalEvent() -> Event {
    final_event()
}

#[allow(non_snake_case)]
pub fn ErrorEvent() -> Event {
    Event::error_event()
}

#[allow(non_snake_case)]
pub fn CompletionEvent(name: impl Into<String>) -> Event {
    Event::completion(name)
}

#[allow(non_snake_case)]
pub fn ObservationEvent(
    source: impl Into<String>,
    occurrence: impl Into<String>,
    observed: Event,
) -> Event {
    observation_event(source, occurrence, observed)
}
