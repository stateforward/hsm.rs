// Demonstration of the new macro-based kind system
use stateforward_hsm::{KindValue, is_kind, kind, make_kind};

fn main() {
    println!("=== Rust HSM Kind System Demo ===");
    println!("Similar to C++ template-based kind system but using Rust macros\n");

    // Show basic kind creation
    println!("1. Basic kind creation:");
    let my_element = make_kind!(30);
    println!("   make_kind!(30) = 0x{:x}", my_element);

    // Show inheritance
    println!("\n2. Kind inheritance:");
    let my_named = make_kind!(31, kind::ELEMENT);
    println!("   make_kind!(31, ELEMENT) = 0x{:x}", my_named);
    println!(
        "   is_kind(my_named, ELEMENT) = {}",
        is_kind(my_named, kind::ELEMENT)
    );

    // Show multiple inheritance
    println!("\n3. Multiple inheritance (like STATE_MACHINE):");
    let custom_state_machine = make_kind!(32, kind::STATE, kind::CONCURRENT);
    println!(
        "   make_kind!(32, STATE, CONCURRENT) = 0x{:x}",
        custom_state_machine
    );
    println!(
        "   is_kind(custom, STATE) = {}",
        is_kind(custom_state_machine, kind::STATE)
    );
    println!(
        "   is_kind(custom, CONCURRENT) = {}",
        is_kind(custom_state_machine, kind::CONCURRENT)
    );
    println!(
        "   is_kind(custom, VERTEX) = {}",
        is_kind(custom_state_machine, kind::VERTEX)
    );
    println!(
        "   is_kind(custom, NAMED_ELEMENT) = {}",
        is_kind(custom_state_machine, kind::NAMED_ELEMENT)
    );

    // Show existing hierarchy
    println!("\n4. Built-in hierarchy test:");
    println!("   STATE_MACHINE kind = 0x{:x}", kind::STATE_MACHINE);
    println!(
        "   is_kind(STATE_MACHINE, STATE) = {}",
        is_kind(kind::STATE_MACHINE, kind::STATE)
    );
    println!(
        "   is_kind(STATE_MACHINE, CONCURRENT) = {}",
        is_kind(kind::STATE_MACHINE, kind::CONCURRENT)
    );
    println!(
        "   is_kind(STATE_MACHINE, VERTEX) = {}",
        is_kind(kind::STATE_MACHINE, kind::VERTEX)
    );
    println!(
        "   is_kind(STATE_MACHINE, NAMED_ELEMENT) = {}",
        is_kind(kind::STATE_MACHINE, kind::NAMED_ELEMENT)
    );
    println!(
        "   is_kind(STATE_MACHINE, ELEMENT) = {}",
        is_kind(kind::STATE_MACHINE, kind::ELEMENT)
    );

    // Show transition types
    println!("\n5. Transition type hierarchy:");
    println!("   EXTERNAL = 0x{:x}", kind::EXTERNAL);
    println!(
        "   is_kind(EXTERNAL, TRANSITION) = {}",
        is_kind(kind::EXTERNAL, kind::TRANSITION)
    );
    println!(
        "   is_kind(EXTERNAL, NAMED_ELEMENT) = {}",
        is_kind(kind::EXTERNAL, kind::NAMED_ELEMENT)
    );
    println!(
        "   is_kind(EXTERNAL, ELEMENT) = {}",
        is_kind(kind::EXTERNAL, kind::ELEMENT)
    );

    // Show pseudostate hierarchy
    println!("\n6. Pseudostate hierarchy:");
    println!("   CHOICE = 0x{:x}", kind::CHOICE);
    println!(
        "   is_kind(CHOICE, PSEUDOSTATE) = {}",
        is_kind(kind::CHOICE, kind::PSEUDOSTATE)
    );
    println!(
        "   is_kind(CHOICE, VERTEX) = {}",
        is_kind(kind::CHOICE, kind::VERTEX)
    );
    println!(
        "   is_kind(CHOICE, NAMED_ELEMENT) = {}",
        is_kind(kind::CHOICE, kind::NAMED_ELEMENT)
    );

    println!("\n=== Comparison with old function-based approach ===");

    // Show the difference in syntax
    println!("OLD: combine_kinds(make_kind(1), ELEMENT)");
    println!("NEW: make_kind!(1, ELEMENT)");
    println!("     Much cleaner syntax, similar to C++ make_kind<1, ELEMENT>()");

    println!("\nThis macro-based approach provides:");
    println!("• Cleaner syntax similar to C++ templates");
    println!("• Compile-time evaluation");
    println!("• Type-safe inheritance checking");
    println!("• Better ergonomics for defining new kinds");
}
