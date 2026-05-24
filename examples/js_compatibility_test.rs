// Test to verify Rust kind values match JavaScript values exactly
use rust::{is_kind, kind, make_kind};

fn main() {
    println!("=== JavaScript Compatibility Test ===");
    println!("Comparing Rust kind values with JavaScript hsm.js values\n");

    // JavaScript values from hsm.js
    let js_values = [
        ("Null", 0, kind::NULL),
        ("Element", 1, kind::ELEMENT),
        ("Partial (NAMED_ELEMENT)", 258, kind::NAMED_ELEMENT),
        ("Vertex", 259, kind::VERTEX),
        ("Constraint", 260, kind::CONSTRAINT),
        ("Behavior", 261, kind::BEHAVIOR),
        ("Concurrent", 66822, kind::CONCURRENT),
        ("Sequential", 66823, kind::SEQUENTIAL),
        ("StateMachine", 66824, kind::STATE_MACHINE),
        ("Namespace", 265, kind::NAMESPACE),
        ("Attribute", 266, kind::ATTRIBUTE),
        ("State", 151061259, kind::STATE),
        ("Model", 38671682316, kind::MODEL),
        ("Transition", 269, kind::TRANSITION),
        ("Internal", 68878, kind::INTERNAL),
        ("External", 68879, kind::EXTERNAL),
        ("Local", 68880, kind::LOCAL),
        ("Self", 68881, kind::SELF),
        ("Event", 274, kind::EVENT),
        ("CompletionEvent", 70163, kind::COMPLETION_EVENT),
        ("ErrorEvent", 17961748, kind::ERROR_EVENT),
        ("TimeEvent", 70165, kind::TIME_EVENT),
        ("Pseudostate", 66326, kind::PSEUDOSTATE),
        ("Initial", 16979479, kind::INITIAL),
        ("FinalState", 38671682328, kind::FINAL_STATE),
        ("Choice", 16979481, kind::CHOICE),
        ("Junction", 16979482, kind::JUNCTION),
        ("DeepHistory", 16979483, kind::DEEP_HISTORY),
    ];

    let mut matches = 0;
    let mut total = 0;

    for (name, js_value, rust_value) in js_values.iter() {
        total += 1;
        if *js_value == *rust_value {
            matches += 1;
            println!("✓ {} = {} (MATCH)", name, js_value);
        } else {
            println!("✗ {} JS={} Rust={} (MISMATCH)", name, js_value, rust_value);
        }
    }

    println!("\n=== Results ===");
    println!(
        "Matches: {}/{} ({:.1}%)",
        matches,
        total,
        (matches as f64 / total as f64) * 100.0
    );

    if matches != total {
        println!("\n=== Diagnostic Information ===");

        // Show what our macro produces for basic cases
        println!("Basic make_kind! values:");
        println!("  make_kind!(1) = {}", make_kind!(1));
        println!(
            "  make_kind!(2, {}) = {}",
            kind::ELEMENT,
            make_kind!(2, kind::ELEMENT)
        );
        println!(
            "  make_kind!(3, {}) = {}",
            kind::NAMED_ELEMENT,
            make_kind!(3, kind::NAMED_ELEMENT)
        );

        // Show inheritance testing
        println!("\nInheritance tests:");
        println!(
            "  is_kind(NAMED_ELEMENT, ELEMENT) = {}",
            is_kind(kind::NAMED_ELEMENT, kind::ELEMENT)
        );
        println!(
            "  is_kind(VERTEX, NAMED_ELEMENT) = {}",
            is_kind(kind::VERTEX, kind::NAMED_ELEMENT)
        );
        println!(
            "  is_kind(STATE, VERTEX) = {}",
            is_kind(kind::STATE, kind::VERTEX)
        );

        println!("\nBinary representation examples:");
        println!("  ELEMENT = 0x{:x} ({})", kind::ELEMENT, kind::ELEMENT);
        println!(
            "  NAMED_ELEMENT = 0x{:x} ({})",
            kind::NAMED_ELEMENT,
            kind::NAMED_ELEMENT
        );
        println!("  VERTEX = 0x{:x} ({})", kind::VERTEX, kind::VERTEX);
        println!("  STATE = 0x{:x} ({})", kind::STATE, kind::STATE);

        println!("\nJavaScript expected patterns:");
        println!("  Vertex should be 259 = 0x{:x}", 259);
        println!("  Constraint should be 260 = 0x{:x}", 260);
        println!("  Behavior should be 261 = 0x{:x}", 261);
        println!("  We're getting much larger numbers, need to fix the algorithm");
    } else {
        println!(
            "🎉 All values match! The Rust macro system correctly replicates JavaScript values."
        );
    }
}
