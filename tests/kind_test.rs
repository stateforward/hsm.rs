use stateforward_hsm::{is_kind, kind, kind_base, make_kind};

// Test family hierarchy like C++ version
const GRANDMA: u64 = make_kind!(1); // Start from 1, not 0
const GRANDPA: u64 = make_kind!(2); // 
const MOTHER: u64 = make_kind!(3, GRANDMA, GRANDPA); // make_kind(3, grandma, grandpa)
const AUNT: u64 = make_kind!(4, GRANDMA, GRANDPA); // make_kind(4, grandma, grandpa) 
const ME: u64 = make_kind!(5, MOTHER); // make_kind(5, mother)
const COUSIN: u64 = make_kind!(6, AUNT); // make_kind(6, aunt)

#[test]
fn test_kind_inheritance() {
    // Test the same assertions as C++ version
    assert!(is_kind(MOTHER, GRANDMA));
    assert!(is_kind(MOTHER, GRANDPA));
    assert!(is_kind(AUNT, GRANDMA));
    assert!(is_kind(AUNT, GRANDPA));
    assert!(!is_kind(AUNT, MOTHER));
    assert!(is_kind(ME, MOTHER));
    assert!(!is_kind(ME, AUNT));
    assert!(is_kind(ME, GRANDMA));
    assert!(is_kind(ME, GRANDPA));
    assert!(!is_kind(ME, COUSIN));
    assert!(is_kind(COUSIN, AUNT));
    assert!(is_kind(COUSIN, GRANDMA));
    assert!(is_kind(COUSIN, GRANDPA));
    assert!(is_kind(MOTHER, GRANDPA));
    assert!(is_kind(MOTHER, MOTHER));
    assert!(!is_kind(GRANDPA, GRANDMA));
    assert!(!is_kind(GRANDPA, MOTHER));
    assert!(!is_kind(GRANDMA, MOTHER));
    assert!(is_kind(AUNT, GRANDMA));
    assert!(is_kind(AUNT, GRANDPA));
    assert!(!is_kind(AUNT, MOTHER));
    assert!(is_kind(COUSIN, AUNT));
    assert!(!is_kind(COUSIN, MOTHER));
    assert!(is_kind(COUSIN, GRANDMA));
    assert!(kind_base(ME) != AUNT);
    assert!(kind_base(ME) == MOTHER);
}

#[test]
fn test_basic_macro_functionality() {
    // Test basic macro variants
    let simple = make_kind!(42);
    let with_one_base = make_kind!(43, simple);
    let with_two_bases = make_kind!(44, simple, with_one_base);

    assert!(is_kind(with_one_base, simple));
    assert!(is_kind(with_two_bases, simple));
    assert!(is_kind(with_two_bases, with_one_base));
    assert!(!is_kind(simple, with_one_base));
}

#[test]
fn test_pascal_kind_aliases_match_internal_kinds() {
    assert_eq!(stateforward_hsm::ElementKind, kind::ELEMENT);
    assert_eq!(stateforward_hsm::StateKind, kind::STATE);
    assert_eq!(stateforward_hsm::TransitionKind, kind::TRANSITION);
    assert_eq!(stateforward_hsm::EventKind, kind::EVENT);
    assert_eq!(
        stateforward_hsm::CompletionEventKind,
        kind::COMPLETION_EVENT
    );
    assert_eq!(stateforward_hsm::ErrorEventKind, kind::ERROR_EVENT);
    assert_eq!(stateforward_hsm::TimeEventKind, kind::TIME_EVENT);
    assert_eq!(stateforward_hsm::CallEventKind, kind::CALL_EVENT);
    assert_eq!(stateforward_hsm::ChangeEventKind, kind::CHANGE_EVENT);
    assert_eq!(
        stateforward_hsm::SetEventKind,
        stateforward_hsm::ChangeEventKind
    );
    assert_eq!(stateforward_hsm::ObservationKind, kind::OBSERVATION);
    assert_eq!(
        stateforward_hsm::SubmachineStateKind,
        kind::SUBMACHINE_STATE
    );
    assert_eq!(stateforward_hsm::EntryPointKind, kind::ENTRY_POINT);
    assert_eq!(stateforward_hsm::ExitPointKind, kind::EXIT_POINT);
}

#[test]
fn test_pascal_make_kind_and_is_kind_api() {
    let custom_event = stateforward_hsm::MakeKind(&[stateforward_hsm::EventKind]);
    assert!(stateforward_hsm::IsKind(
        custom_event,
        &[stateforward_hsm::EventKind]
    ));
    assert!(stateforward_hsm::IsKind(
        custom_event,
        &[stateforward_hsm::ElementKind]
    ));
    assert!(!stateforward_hsm::IsKind(
        custom_event,
        &[stateforward_hsm::StateKind]
    ));

    let custom_empty = stateforward_hsm::MakeKind(&[]);
    assert!(!stateforward_hsm::IsKind(
        custom_empty,
        &[stateforward_hsm::EventKind]
    ));

    let macro_kind =
        stateforward_hsm::MakeKind!(stateforward_hsm::StateKind, stateforward_hsm::EventKind);
    assert!(stateforward_hsm::IsKind!(
        macro_kind,
        stateforward_hsm::StateKind
    ));
    assert!(stateforward_hsm::IsKind!(
        macro_kind,
        stateforward_hsm::EventKind
    ));
    assert!(!stateforward_hsm::IsKind!(
        macro_kind,
        stateforward_hsm::TransitionKind,
        stateforward_hsm::PseudostateKind
    ));
}

#[test]
fn test_runtime_kind_hierarchy_aliases() {
    assert!(is_kind(
        stateforward_hsm::StateMachineKind,
        stateforward_hsm::ConcurrentKind
    ));
    assert!(is_kind(
        stateforward_hsm::StateMachineKind,
        stateforward_hsm::BehaviorKind
    ));
    assert!(is_kind(
        stateforward_hsm::StateMachineKind,
        stateforward_hsm::NamespaceKind
    ));
    assert!(is_kind(
        stateforward_hsm::SequentialKind,
        stateforward_hsm::BehaviorKind
    ));
    assert!(is_kind(
        stateforward_hsm::ChangeEventKind,
        stateforward_hsm::EventKind
    ));
    assert!(is_kind(
        stateforward_hsm::ObservationKind,
        stateforward_hsm::ElementKind
    ));
}

#[test]
fn test_macro_syntax_equivalence() {
    // Test that our macro syntax works like C++ template syntax
    println!("GRANDMA = 0x{:x}", GRANDMA); // Should be 1 
    println!("GRANDPA = 0x{:x}", GRANDPA); // Should be 2
    println!("MOTHER = 0x{:x}", MOTHER); // Should have both grandparents
    println!("ME = 0x{:x}", ME); // Should inherit through mother

    // Debug problematic case
    println!(
        "is_kind(GRANDPA={:x}, GRANDMA={:x}) = {}",
        GRANDPA,
        GRANDMA,
        is_kind(GRANDPA, GRANDMA)
    );
    println!(
        "is_kind(MOTHER={:x}, GRANDMA={:x}) = {}",
        MOTHER,
        GRANDMA,
        is_kind(MOTHER, GRANDMA)
    );
    println!(
        "is_kind(MOTHER={:x}, GRANDPA={:x}) = {}",
        MOTHER,
        GRANDPA,
        is_kind(MOTHER, GRANDPA)
    );

    // Verify the chain works
    assert!(is_kind(ME, MOTHER));
    assert!(is_kind(ME, GRANDMA));
    assert!(is_kind(ME, GRANDPA));
}
