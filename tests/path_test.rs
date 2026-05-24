use rust::path::*;

#[test]
fn test_join_basic() {
    // Test basic joins
    assert_eq!(join("/", "child"), "/child");
    assert_eq!(join("/parent", "child"), "/parent/child");
    assert_eq!(join("/a/b", "c"), "/a/b/c");

    // Test with empty base
    assert_eq!(join("", "child"), "child");

    // Test with absolute path (should ignore base)
    assert_eq!(join("/parent", "/absolute"), "/absolute");
    assert_eq!(join("/a/b/c", "/x/y/z"), "/x/y/z");

    // Test edge cases
    assert_eq!(join("/", ""), "/");
    assert_eq!(join("", ""), "");
}

#[test]
fn test_dirname() {
    // Test basic dirname
    assert_eq!(dirname("/parent/child"), "/parent");
    assert_eq!(dirname("/a/b/c"), "/a/b");
    assert_eq!(dirname("/root"), "");

    // Test root and empty
    assert_eq!(dirname("/"), "");
    assert_eq!(dirname(""), "");

    // Test single level
    assert_eq!(dirname("file"), "");

    // Test paths with multiple levels
    assert_eq!(dirname("/a/b/c/d/e"), "/a/b/c/d");
}

#[test]
fn test_basename() {
    // Test basic basename
    assert_eq!(basename("/parent/child"), "child");
    assert_eq!(basename("/a/b/c"), "c");
    assert_eq!(basename("/root"), "root");

    // Test root
    assert_eq!(basename("/"), "");

    // Test no slash
    assert_eq!(basename("file"), "file");

    // Test empty
    assert_eq!(basename(""), "");

    // Test complex paths
    assert_eq!(basename("/very/long/path/to/file"), "file");
}

#[test]
fn test_is_ancestor_or_equal() {
    // Test equal paths
    assert!(is_ancestor_or_equal("/a/b", "/a/b"));
    assert!(is_ancestor_or_equal("/", "/"));
    assert!(is_ancestor_or_equal("", ""));

    // Test ancestor relationships
    assert!(is_ancestor_or_equal("/a", "/a/b"));
    assert!(is_ancestor_or_equal("/a", "/a/b/c"));
    assert!(is_ancestor_or_equal("/", "/anything"));
    assert!(is_ancestor_or_equal("/", "/a/b/c"));

    // Test non-ancestor relationships
    assert!(!is_ancestor_or_equal("/a/b", "/a"));
    assert!(!is_ancestor_or_equal("/a/b", "/c/d"));
    assert!(!is_ancestor_or_equal("/x", "/y"));

    // Test edge cases with empty and root
    assert!(!is_ancestor_or_equal("", "/"));
    assert!(!is_ancestor_or_equal("/a", "/"));
    assert!(!is_ancestor_or_equal("/a", ""));

    // Test similar but not ancestor paths
    assert!(!is_ancestor_or_equal("/abc", "/abcd"));
    assert!(!is_ancestor_or_equal("/a/bc", "/a/bcd"));
}

#[test]
fn test_lca_basic() {
    // Test same directory
    assert_eq!(lca("/a/b", "/a/c"), "/a");
    assert_eq!(lca("/x/y/z", "/x/y/w"), "/x/y");

    // Test different depths - when one is ancestor of other
    assert_eq!(lca("/a/b/c", "/a"), "/a");
    assert_eq!(lca("/a", "/a/b/c"), "/a");

    // Test no common ancestor except root
    assert_eq!(lca("/a/b", "/c/d"), "");

    // Test identical paths (returns parent)
    assert_eq!(lca("/a/b/c", "/a/b/c"), "/a/b");

    // Test empty paths
    assert_eq!(lca("", "/a/b"), "/a/b");
    assert_eq!(lca("/a/b", ""), "/a/b");
    assert_eq!(lca("", ""), "");
}

#[test]
fn test_lca_complex() {
    // Test complex hierarchies
    assert_eq!(lca("/a/b/c/d", "/a/b/e/f"), "/a/b");
    assert_eq!(lca("/a/b/c/d/e", "/a/b/c/f/g"), "/a/b/c");

    // Test siblings at root
    assert_eq!(lca("/a", "/b"), "");
    assert_eq!(lca("/abc", "/def"), "");
}

#[test]
fn test_event_matches() {
    // Test exact matching
    assert!(event_matches("click", "click"));
    assert!(event_matches("", ""));
    assert!(event_matches("EVENT_NAME", "EVENT_NAME"));

    // Test non-matching
    assert!(!event_matches("click", "hover"));
    assert!(!event_matches("a", "b"));
    assert!(!event_matches("", "something"));
    assert!(!event_matches("something", ""));

    // Test case sensitivity
    assert!(!event_matches("Click", "click"));
    assert!(!event_matches("EVENT", "event"));
}

#[test]
fn test_path_operations_combined() {
    // Test combining multiple path operations
    let base = "/root/parent";
    let child = "child";
    let full_path = join(base, child);

    assert_eq!(full_path, "/root/parent/child");
    assert_eq!(dirname(&full_path), "/root/parent");
    assert_eq!(basename(&full_path), "child");

    // Test with absolute path
    let abs_path = "/absolute/path";
    let joined = join("/ignored", abs_path);
    assert_eq!(joined, abs_path);
}

#[test]
fn test_state_machine_paths() {
    // Test typical state machine path scenarios
    let machine = "/TestMachine";
    let state1 = join(machine, "State1");
    let state2 = join(machine, "State2");
    let nested = join(&state1, "Nested");

    assert_eq!(state1, "/TestMachine/State1");
    assert_eq!(state2, "/TestMachine/State2");
    assert_eq!(nested, "/TestMachine/State1/Nested");

    assert!(is_ancestor_or_equal(machine, &state1));
    assert!(is_ancestor_or_equal(machine, &nested));
    assert!(is_ancestor_or_equal(&state1, &nested));
    assert!(!is_ancestor_or_equal(&state1, &state2));

    assert_eq!(lca(&state1, &state2), machine);
    assert_eq!(lca(&nested, &state2), machine);
}

#[test]
fn test_empty_and_root_edge_cases() {
    // Comprehensive edge case testing

    // dirname edge cases
    assert_eq!(dirname("/"), "");
    assert_eq!(dirname(""), "");
    assert_eq!(dirname("no_slash"), "");

    // basename edge cases
    assert_eq!(basename("/"), "");
    assert_eq!(basename(""), "");

    // join edge cases
    assert_eq!(join("", ""), "");
    assert_eq!(join("/", ""), "/");
    assert_eq!(join("", "/abs"), "/abs");

    // is_ancestor_or_equal edge cases
    assert!(is_ancestor_or_equal("/", "/"));
    assert!(is_ancestor_or_equal("", ""));
    assert!(!is_ancestor_or_equal("", "/"));

    // lca edge cases
    assert_eq!(lca("/", "/"), "");
    assert_eq!(lca("", ""), "");
    assert_eq!(lca("/a", "/a"), "");
}
