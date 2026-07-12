use stateforward_hsm::event::*;
use stateforward_hsm::kind;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

#[test]
fn test_event_creation() {
    let event = Event::new("test_event");
    assert_eq!(event.name, "test_event");
    assert_eq!(event.qualified_name, "test_event");
    assert_eq!(event.kind, kind::EVENT);
    assert!(event.data.is_none());
}

#[test]
fn test_event_with_data() {
    #[derive(Debug)]
    struct TestData {
        value: i32,
        message: String,
    }

    let data = TestData {
        value: 42,
        message: "test".to_string(),
    };

    let event = Event::new("data_event").with_data(data);
    assert_eq!(event.name, "data_event");
    assert!(event.data.is_some());

    // Test data access
    if let Some(data_arc) = &event.data {
        if let Some(test_data) = data_arc.downcast_ref::<TestData>() {
            assert_eq!(test_data.value, 42);
            assert_eq!(test_data.message, "test");
        } else {
            panic!("Failed to downcast event data");
        }
    }
}

#[test]
fn test_event_data_thread_safety() {
    // Test that event data can be shared across threads
    use std::thread;

    let event = Event::new("shared_event").with_data(100i32);
    let event_arc = Arc::new(event);

    let event_clone = event_arc.clone();
    let handle = thread::spawn(move || {
        if let Some(data) = &event_clone.data {
            if let Some(value) = data.downcast_ref::<i32>() {
                assert_eq!(*value, 100);
            }
        }
    });

    handle.join().unwrap();

    // Original event should still have data
    if let Some(data) = &event_arc.data {
        if let Some(value) = data.downcast_ref::<i32>() {
            assert_eq!(*value, 100);
        }
    }
}

#[test]
fn test_completion_event() {
    let event = Event::completion("test_completion");
    assert_eq!(event.name, "test_completion");
    assert_eq!(event.qualified_name, "test_completion");
    assert_eq!(event.kind, kind::COMPLETION_EVENT);
    assert!(event.data.is_none());
}

#[test]
fn test_time_event() {
    let event = Event::time_event("timer_expired");
    assert_eq!(event.name, "timer_expired");
    assert_eq!(event.qualified_name, "timer_expired");
    assert_eq!(event.kind, kind::TIME_EVENT);
    assert!(event.data.is_none());
}

#[test]
fn test_error_event() {
    let event = Event::error_event();
    assert_eq!(event.name, "hsm/error");
    assert_eq!(event.qualified_name, "hsm/error");
    assert_eq!(event.kind, kind::ERROR_EVENT);
    assert!(event.data.is_none());
}

#[test]
fn test_standard_events() {
    let initial = initial_event();
    assert_eq!(initial.name, "hsm/initial");
    assert_eq!(initial.kind, kind::COMPLETION_EVENT);

    let final_ev = final_event();
    assert_eq!(final_ev.name, "hsm/final");
    assert_eq!(final_ev.kind, kind::COMPLETION_EVENT);

    let any = any_event();
    assert_eq!(AnyEvent, "*");
    assert_eq!(any.name, AnyEvent);
    assert_eq!(any.kind, kind::EVENT);
}

#[test]
fn test_pascal_runtime_event_constructors() {
    let initial = InitialEvent();
    assert_eq!(initial.name, "hsm/initial");
    assert_eq!(initial.qualified_name, "hsm/initial");
    assert_eq!(initial.kind, kind::COMPLETION_EVENT);

    let final_event = FinalEvent();
    assert_eq!(final_event.name, "hsm/final");
    assert_eq!(final_event.qualified_name, "hsm/final");
    assert_eq!(final_event.kind, kind::COMPLETION_EVENT);

    let error = ErrorEvent();
    assert_eq!(error.name, "hsm/error");
    assert_eq!(error.qualified_name, "hsm/error");
    assert_eq!(error.kind, kind::ERROR_EVENT);

    let completion = CompletionEvent("done");
    assert_eq!(completion.name, "done");
    assert_eq!(completion.qualified_name, "done");
    assert_eq!(completion.kind, kind::COMPLETION_EVENT);
}

#[test]
fn test_observation_event_carries_observed_event_payload() {
    let observed = Event::new("go").with_data(7i32);
    let observation = ObservationEvent("/Observed/idle/entry", "behavior", observed.clone());

    assert_eq!(observation.name, OBSERVATION_EVENT_NAME);
    assert_eq!(observation.qualified_name, OBSERVATION_EVENT_NAME);
    assert_eq!(observation.kind, kind::EVENT);

    let data = observation
        .get_data::<ObservationData>()
        .expect("observation event should carry ObservationData");
    assert_eq!(data.Occurrence, "behavior");
    assert_eq!(data.Event.name, observed.name);
    assert_eq!(data.Event.qualified_name, observed.qualified_name);
    assert_eq!(data.Event.kind, observed.kind);
    assert!(data.Event.get_data::<i32>().is_some());
    assert!(data.Time.duration_since(UNIX_EPOCH).is_ok());
}

#[test]
fn test_event_clone() {
    let event = Event::new("test").with_data(vec![1, 2, 3]);
    let cloned = event.clone();

    assert_eq!(event.name, cloned.name);
    assert_eq!(event.qualified_name, cloned.qualified_name);
    assert_eq!(event.kind, cloned.kind);

    // Data should be shared (Arc)
    assert!(Arc::ptr_eq(
        event.data.as_ref().unwrap(),
        cloned.data.as_ref().unwrap()
    ));
}

#[test]
fn test_event_debug() {
    let event = Event::new("debug_test");
    let debug_str = format!("{:?}", event);

    assert!(debug_str.contains("Event"));
    assert!(debug_str.contains("debug_test"));
    assert!(debug_str.contains("kind"));
}

#[test]
fn test_event_data_types() {
    // Test various data types

    // String data
    let str_event = Event::new("str").with_data("hello".to_string());
    if let Some(data) = &str_event.data {
        assert_eq!(data.downcast_ref::<String>().unwrap(), "hello");
    }

    // Vector data
    let vec_event = Event::new("vec").with_data(vec![1, 2, 3, 4, 5]);
    if let Some(data) = &vec_event.data {
        assert_eq!(
            data.downcast_ref::<Vec<i32>>().unwrap(),
            &vec![1, 2, 3, 4, 5]
        );
    }

    // Tuple data
    let tuple_event = Event::new("tuple").with_data((42, "answer"));
    if let Some(data) = &tuple_event.data {
        let tuple = data.downcast_ref::<(i32, &str)>().unwrap();
        assert_eq!(tuple.0, 42);
        assert_eq!(tuple.1, "answer");
    }

    // Option data
    let option_event = Event::new("option").with_data(Some(100));
    if let Some(data) = &option_event.data {
        assert_eq!(data.downcast_ref::<Option<i32>>().unwrap(), &Some(100));
    }
}

#[test]
fn test_event_data_downcast_fail() {
    let event = Event::new("test").with_data(42i32);

    if let Some(data) = &event.data {
        // Try to downcast to wrong type
        assert!(data.downcast_ref::<String>().is_none());
        assert!(data.downcast_ref::<f64>().is_none());

        // Correct type should work
        assert!(data.downcast_ref::<i32>().is_some());
    }
}

#[test]
fn test_event_builder_pattern() {
    // Test that with_data consumes and returns self
    let event = Event::new("builder").with_data("some data");

    assert_eq!(event.name, "builder");
    assert!(event.data.is_some());
}

#[test]
fn test_event_kinds_hierarchy() {
    use stateforward_hsm::kind::is_kind;

    // Regular event
    let event = Event::new("test");
    assert!(is_kind(event.kind, kind::EVENT));
    assert!(is_kind(event.kind, kind::ELEMENT));

    // Completion event
    let completion = Event::completion("done");
    assert!(is_kind(completion.kind, kind::COMPLETION_EVENT));
    assert!(is_kind(completion.kind, kind::EVENT));
    assert!(is_kind(completion.kind, kind::ELEMENT));

    // Error event
    let error = Event::error_event();
    assert!(is_kind(error.kind, kind::ERROR_EVENT));
    assert!(is_kind(error.kind, kind::COMPLETION_EVENT));
    assert!(is_kind(error.kind, kind::EVENT));
    assert!(is_kind(error.kind, kind::ELEMENT));

    // Time event
    let time = Event::time_event("timeout");
    assert!(is_kind(time.kind, kind::TIME_EVENT));
    assert!(is_kind(time.kind, kind::EVENT));
    assert!(is_kind(time.kind, kind::ELEMENT));
}

#[test]
fn test_event_names() {
    // Test different name formats
    let simple = Event::new("simple");
    assert_eq!(simple.name, "simple");
    assert_eq!(simple.qualified_name, "simple");

    let underscore = Event::new("under_score_name");
    assert_eq!(underscore.name, "under_score_name");

    let caps = Event::new("CAPS_EVENT");
    assert_eq!(caps.name, "CAPS_EVENT");

    let dotted = Event::new("namespace.event");
    assert_eq!(dotted.name, "namespace.event");
}
