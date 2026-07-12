use stateforward_hsm::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
struct HookInstance;

impl Instance for HookInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[test]
fn validator_hook_last_wins_and_sees_built_model() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let first_calls = calls.clone();
    let second_calls = calls.clone();

    let model: Model<HookInstance> = Define(
        "ValidatorHookMachine",
        vec![
            Validator(move |_model: &Model<HookInstance>| -> Result<()> {
                first_calls.lock().unwrap().push("first".to_string());
                Err(HsmError::Validation(
                    "first validator should not run".to_string(),
                ))
            }),
            Validator(move |model: &Model<HookInstance>| -> Result<()> {
                assert!(model.get_state("/ValidatorHookMachine/ready").is_some());
                second_calls.lock().unwrap().push("second".to_string());
                Ok(())
            }),
            Initial(vec![Target("ready")]),
            State("ready", vec![]),
        ],
    );

    assert_eq!(*calls.lock().unwrap(), vec!["second".to_string()]);
    validate(&model).unwrap();
    assert_eq!(
        *calls.lock().unwrap(),
        vec!["second".to_string(), "second".to_string()]
    );
}

#[test]
fn validator_hook_error_aborts_model_definition() {
    let error = std::panic::catch_unwind(|| {
        let _model: Model<HookInstance> = Define(
            "ValidatorErrorMachine",
            vec![
                Validator(|_model: &Model<HookInstance>| -> Result<()> {
                    Err(HsmError::Validation("custom validator error".to_string()))
                }),
                Initial(vec![Target("ready")]),
                State("ready", vec![]),
            ],
        );
    });

    assert!(error.is_err());
}

#[test]
fn validate_uses_custom_validator_after_definition() {
    let fail_next_validation = Arc::new(AtomicBool::new(false));
    let validator_flag = fail_next_validation.clone();
    let model: Model<HookInstance> = Define(
        "ValidateHookMachine",
        vec![
            Validator(move |_model: &Model<HookInstance>| -> Result<()> {
                if validator_flag.load(Ordering::SeqCst) {
                    Err(HsmError::Validation("later validation error".to_string()))
                } else {
                    Ok(())
                }
            }),
            Initial(vec![Target("ready")]),
            State("ready", vec![]),
        ],
    );

    fail_next_validation.store(true, Ordering::SeqCst);
    let error = validate(&model).unwrap_err();
    assert!(matches!(
        error,
        HsmError::Validation(message) if message == "later validation error"
    ));
}

#[test]
fn finalizer_hook_last_wins_and_can_delegate_to_default() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let first_calls = calls.clone();
    let second_calls = calls.clone();

    let model: Model<HookInstance> = Define(
        "FinalizerHookMachine",
        vec![
            Finalizer(move |_model: &mut Model<HookInstance>| {
                first_calls.lock().unwrap().push("first".to_string());
            }),
            Finalizer(move |model: &mut Model<HookInstance>| {
                second_calls.lock().unwrap().push("second".to_string());
                DefaultModelFinalizer.finalize(model);
            }),
            Initial(vec![Target("ready")]),
            State("ready", vec![Transition(vec![On("go"), Target("../done")])]),
            State("done", vec![]),
        ],
    );

    assert_eq!(*calls.lock().unwrap(), vec!["second".to_string()]);
    let ready_events = model
        .transition_map
        .get("/FinalizerHookMachine/ready")
        .expect("default finalizer should build transition map");
    assert!(ready_events.contains_key("go"));
}

#[test]
fn redefine_preserves_and_can_replace_model_hooks() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let base_calls = calls.clone();
    let replacement_calls = calls.clone();

    let base: Model<HookInstance> = Define(
        "BaseHookMachine",
        vec![
            Validator(move |_model: &Model<HookInstance>| -> Result<()> {
                base_calls.lock().unwrap().push("base".to_string());
                Ok(())
            }),
            Initial(vec![Target("ready")]),
            State("ready", vec![]),
        ],
    );
    assert_eq!(*calls.lock().unwrap(), vec!["base".to_string()]);

    let preserved = RedefineAs(&base, "PreservedHookMachine", vec![]);
    validate(&preserved).unwrap();
    assert_eq!(
        *calls.lock().unwrap(),
        vec!["base".to_string(), "base".to_string(), "base".to_string()]
    );

    let replaced = RedefineAs(
        &base,
        "ReplacedHookMachine",
        vec![Validator(
            move |_model: &Model<HookInstance>| -> Result<()> {
                replacement_calls
                    .lock()
                    .unwrap()
                    .push("replacement".to_string());
                Ok(())
            },
        )],
    );
    validate(&replaced).unwrap();
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            "base".to_string(),
            "base".to_string(),
            "base".to_string(),
            "replacement".to_string(),
            "replacement".to_string()
        ]
    );
}
