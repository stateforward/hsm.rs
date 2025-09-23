#[test]
fn test_parent_resolution() {
    use rust::path::{dirname, join, lca};
    
    let child2 = "/AncestorActiveMachine/parent1/child2";
    let parent = dirname(child2);
    println\!("child2: {}", child2);
    println\!("parent (..): {}", parent);
    println\!("lca({}, {}): {}", child2, parent, lca(child2, parent));
    
    // Test calculate_path_static behavior
    let source = child2;
    let target = parent;
    let common_ancestor = lca(source, target);
    
    println\!("\nTransition from {} to {}", source, target);
    println\!("LCA: {}", common_ancestor);
    
    // Exit path
    let mut exit = Vec::new();
    let mut current = source.to_string();
    while current \!= common_ancestor && \!current.is_empty() {
        println\!("Exit: {}", current);
        exit.push(current.clone());
        current = dirname(&current).to_string();
    }
    
    // Enter path
    let mut enter = Vec::new();
    let mut path_to_target = Vec::new();
    let mut current = target.to_string();
    while current \!= common_ancestor && \!current.is_empty() {
        println\!("Will enter: {}", current);
        path_to_target.push(current.clone());
        current = dirname(&current).to_string();
    }
    path_to_target.reverse();
    enter = path_to_target;
    
    println\!("\nFinal exit path: {:?}", exit);
    println\!("Final enter path: {:?}", enter);
}
