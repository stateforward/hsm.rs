# 🦀 Rust HSM - Hierarchical State Machine

A high-performance, memory-safe Hierarchical State Machine implementation in Rust with async/await support, leveraging Rust's type system and zero-cost abstractions for optimal performance and safety.

## ✨ Key Features

* **🛡️ Memory Safe**: Zero unsafe code, leveraging Rust's ownership system
* **⚡ Zero-Cost Abstractions**: Compile-time optimizations with no runtime overhead
* **🔧 Type-Safe API**: Strong type guarantees and compile-time validation
* **🚀 High Performance**: Optimized transition paths and lookup tables
* **🔄 Async/Await**: Full tokio integration for concurrent state machines
* **🎯 Macro-Based Builders**: Ergonomic API with `define!`, `state!`, `transition!` macros
* **📊 Kind System**: Compile-time type hierarchy system inspired by C++ templates
* **✅ Comprehensive Tests**: 19+ test suites covering all functionality

## 📦 Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
rust = "0.1.0"

[dev-dependencies]
tokio = { version = "1.0", features = ["full"] }
```

## 🚀 Quick Start

### Basic State Machine

```rust
use rust::*;

// Define your instance type
#[derive(Debug)]
struct MyInstance {
    counter: i32,
    log: Vec<String>,
}

impl Instance for MyInstance {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let instance = MyInstance { 
        counter: 0, 
        log: Vec::new() 
    };
    let ctx = Context::new();

    // Define the state machine using macros
    let model = define!("SimpleMachine",
        initial!(target!("idle")),
        state!("idle",
            entry!(|_ctx, inst: &mut MyInstance, _event| {
                inst.log.push("Entered idle".to_string());
            }),
            transition!(on!("start"), target!("../running"))
        ),
        state!("running",
            entry!(|_ctx, inst: &mut MyInstance, _event| {
                inst.log.push("Entered running".to_string());
            }),
            transition!(on!("stop"), target!("../idle"))
        )
    );

    // Start the state machine
    let hsm = start(&ctx, instance, model)?;
    hsm.start().await;

    println!("Current state: {}", hsm.state());
    // Output: Current state: /SimpleMachine/idle

    // Dispatch events
    hsm.dispatch(&ctx, Event::new("start")).await;
    println!("After start: {}", hsm.state());
    // Output: After start: /SimpleMachine/running

    hsm.dispatch(&ctx, Event::new("stop")).await;
    println!("After stop: {}", hsm.state());
    // Output: After stop: /SimpleMachine/idle

    Ok(())
}
```

## 🎯 API Overview

### Core Builder Functions

#### State Definitions

```rust
// Define a simple state
state!("stateName")

// Define a state with behaviors
state!("stateName",
    entry!(entry_fn),
    exit!(exit_fn),
    activity!(activity_fn),
    transition!(on!("event"), target!("../targetState"))
)

// Final state
final_state!("done")

// Choice pseudostate
choice!("decision",
    transition!(guard!(guard_fn), target!("../option1")),
    transition!(target!("../option2")) // Guardless fallback
)

// Initial pseudostate
initial!(target!("defaultState"))
```

#### Transition Conditions

```rust
// Event trigger
on!("eventName")

// Guard condition - SYNCHRONOUS, returns bool
guard!(|_ctx, inst: &MyInstance, _event| inst.counter > 5)

// Transition effect - SYNCHRONOUS
effect!(|_ctx, inst: &mut MyInstance, _event| {
    inst.counter += 1;
})

// Target state
target!("../targetStatePath")
```

#### Lifecycle Actions

```rust
// Entry action (executed when entering a state) - SYNCHRONOUS
entry!(|_ctx, inst: &mut MyInstance, _event| {
    inst.log.push("Entered state".to_string());
})

// Exit action (executed when exiting a state) - SYNCHRONOUS
exit!(|_ctx, inst: &mut MyInstance, _event| {
    inst.log.push("Exited state".to_string());
})

// Activity (runs while in state, cancelled on exit) - ASYNC
activity!(|_ctx, inst: &mut MyInstance, _event| {
    inst.log.push("Activity running".to_string());
    Box::pin(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    })
})
```

#### Timer-Based Transitions

```rust
// One-time delay
after!(|_ctx, inst: &MyInstance, _event| {
    std::time::Duration::from_secs(inst.timeout_seconds)
})

// Periodic timer
every!(|_ctx, inst: &MyInstance, _event| {
    std::time::Duration::from_millis(100)
})
```

## 🏗️ Architecture

### Module Structure

```
rust/
├── src/
│   ├── lib.rs           # Main library entry, re-exports
│   ├── kind.rs          # Macro-based type hierarchy system
│   ├── path.rs          # Path resolution utilities
│   ├── context.rs       # Execution context with cancellation
│   ├── event.rs         # Event system with type-safe data
│   ├── element.rs       # HSM element types (State, Transition, etc.)
│   ├── model.rs         # State machine model and lookup tables
│   ├── queue.rs         # Event queue management
│   ├── hsm_impl.rs      # HSM runtime implementation
│   ├── builder.rs       # Builder pattern for model construction
│   ├── macro_builders.rs # Macro implementations
│   ├── macros.rs        # Macro definitions
│   └── error.rs         # Error types
├── examples/
│   ├── kind_demo.rs     # Kind system demonstration
│   └── js_compatibility_test.rs
└── tests/
    ├── basic_functionality_test.rs
    ├── hierarchical_states_test.rs
    ├── guard_conditions_test.rs
    ├── choice_states_test.rs
    └── ... 19 total test files
```

### Kind System

Rust HSM features a unique **macro-based kind system** that provides compile-time type hierarchy checking, similar to C++ templates:

```rust
use rust::{make_kind, kind, is_kind};

// Create custom kinds with inheritance
let custom_kind = make_kind!(30, kind::ELEMENT);

// Check kind relationships
println!("{}", is_kind(kind::STATE_MACHINE, kind::STATE));     // true
println!("{}", is_kind(kind::CHOICE, kind::PSEUDOSTATE));      // true
println!("{}", is_kind(kind::EXTERNAL, kind::TRANSITION));     // true

// Built-in kind hierarchy:
// ELEMENT
// ├── NAMESPACE
// ├── VERTEX
// │   ├── PSEUDOSTATE
// │   │   ├── INITIAL
// │   │   └── CHOICE
// │   └── STATE
// │       └── FINAL_STATE
// ├── TRANSITION
// │   ├── INTERNAL
// │   ├── EXTERNAL
// │   ├── LOCAL
// │   └── SELF
// └── EVENT
//     ├── COMPLETION_EVENT
//     │   └── ERROR_EVENT
//     └── TIME_EVENT
```

See [`examples/kind_demo.rs`](examples/kind_demo.rs) for a comprehensive demonstration.

## 📚 Advanced Examples

### Hierarchical States

```rust
let model = define!("GameMachine",
    initial!(target!("menu")),
    state!("menu",
        transition!(on!("start"), target!("../game"))
    ),
    state!("game",
        initial!(target!("playing")),
        entry!(|_ctx, inst: &mut GameInstance, _event| {
            inst.log.push("Game started".to_string());
        }),
        state!("playing",
            transition!(on!("pause"), target!("../paused")),
            transition!(on!("game_over"), target!("../../gameOver"))
        ),
        state!("paused",
            transition!(on!("resume"), target!("../playing"))
        )
    ),
    state!("gameOver",
        transition!(on!("restart"), target!("../menu"))
    )
);
```

### Guards and Effects

```rust
// Guard - SYNCHRONOUS, returns bool
fn can_transition(_ctx: &Context, inst: &MyInstance, _event: &Event) -> bool {
    inst.counter < 10
}

// Effect - SYNCHRONOUS
fn increment_counter(_ctx: &Context, inst: &mut MyInstance, _event: &Event) {
    inst.counter += 1;
}

let model = define!("ConditionalMachine",
    initial!(target!("counting")),
    state!("counting",
        transition!(
            on!("increment"),
            guard!(can_transition),
            effect!(increment_counter),
            target!(".")
        ),
        transition!(
            on!("increment"),
            target!("../maxed")
        )
    ),
    state!("maxed")
);
```

### Choice Pseudostates

```rust
let model = define!("DecisionMachine",
    initial!(target!("input")),
    state!("input",
        transition!(on!("process"), target!("../decision"))
    ),
    choice!("decision",
        transition!(
            guard!(|_ctx, inst: &MyInstance, _event| inst.value > 10),
            target!("../high")
        ),
        transition!(
            guard!(|_ctx, inst: &MyInstance, _event| inst.value > 0),
            target!("../medium")
        ),
        transition!(target!("../low")) // Guardless fallback required
    ),
    state!("high"),
    state!("medium"),
    state!("low")
);

// Validate choice states have guardless fallback
validate(&model)?;
```

### Events with Type-Safe Data

```rust
#[derive(Debug)]
struct MyData {
    value: i32,
    message: String,
}

// Create event with data
let event = Event::new("process").with_data(MyData {
    value: 42,
    message: "Hello".to_string(),
});

// Access data in handlers - SYNCHRONOUS
fn handle_event(_ctx: &Context, inst: &mut MyInstance, event: &Event) {
    if let Some(data) = event.get_data::<MyData>() {
        inst.log.push(format!("Received: {} - {}", data.value, data.message));
    }
}
```

### Internal vs External Transitions

```rust
let model = define!("TransitionTypesMachine",
    initial!(target!("active")),
    state!("active",
        entry!(entry_fn),
        exit!(exit_fn),
        
        // Internal transition - no exit/entry
        transition!(on!("internal"), effect!(effect_fn)),
        
        // External transition - exit and re-enter
        transition!(on!("external"), target!("."), effect!(effect_fn))
    )
);

// Internal: Only effect_fn runs
// External: exit_fn → effect_fn → entry_fn
```

### Context Cancellation

```rust
let ctx = Context::new();

// Long-running activity
activity!(|ctx, inst: &mut MyInstance, _event| {
    Box::pin(async move {
        loop {
            if ctx.is_cancelled() {
                break; // Gracefully exit
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
})

// Cancel all activities
ctx.cancel();
```

## 🧪 Testing

Run the comprehensive test suite:

```bash
# Run all tests
cargo test

# Run specific test suite
cargo test hierarchical_states

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_basic_state_machine_with_simple_transitions
```

### Test Coverage

* ✅ Basic functionality (transitions, lifecycle)
* ✅ Hierarchical states (nested states, path resolution)
* ✅ Guard conditions (synchronous, async)
* ✅ Choice pseudostates (with validation)
* ✅ Entry/exit/activity actions
* ✅ Internal vs external transitions
* ✅ Event handling with type-safe data
* ✅ Context and cancellation
* ✅ Validation (choice fallbacks, final states)
* ✅ Timer transitions (after, every)
* ✅ Path resolution
* ✅ Kind system hierarchy
* ✅ Queue management

## 🎯 Performance Characteristics

### Zero-Cost Abstractions

* **Compile-time optimizations**: Transition paths calculated at model build time
* **Efficient lookups**: Pre-built transition and deferred event tables
* **Minimal allocations**: Smart use of `Rc`/`Arc` for shared data
* **Stack allocation**: Events and contexts are stack-allocated where possible

### Benchmarking

```bash
# Run benchmarks
cargo bench

# Profile with flamegraph
cargo install flamegraph
cargo flamegraph --bench hsm_bench
```

## 🔧 Configuration

### Features

```toml
[dependencies]
rust = { version = "0.1.0", default-features = false }

[features]
default = ["tokio"]
tokio = ["dep:tokio"]  # Async/await support
```

### Without async support

```rust
// Synchronous usage (no tokio)
let model = define!("SyncMachine", /* ... */);
let hsm = start(&ctx, instance, model)?;
// ... use without .await
```

## 🛡️ Error Handling

```rust
use rust::{Result, HsmError};

// Validation errors
match validate(&model) {
    Ok(_) => println!("Model is valid"),
    Err(HsmError::Validation(msg)) => eprintln!("Validation error: {}", msg),
    Err(e) => eprintln!("Error: {:?}", e),
}

// Runtime errors
match hsm.dispatch(&ctx, event).await {
    Ok(_) => println!("Event dispatched"),
    Err(HsmError::InvalidState(msg)) => eprintln!("State error: {}", msg),
    Err(e) => eprintln!("Error: {:?}", e),
}
```

## 🔑 Synchronous vs Async

Understanding the execution model is crucial:

### Synchronous Functions (Immediate Execution)

* **Entry actions**: Execute immediately when entering a state
* **Exit actions**: Execute immediately when exiting a state
* **Guards**: Evaluate immediately to determine if transition is allowed
* **Effects**: Execute immediately during transition

```rust
// Synchronous - no async/await needed
entry!(|_ctx, inst: &mut MyInstance, _event| {
    inst.counter += 1;  // Executes immediately
})

guard!(|_ctx, inst: &MyInstance, _event| {
    inst.counter > 5  // Evaluates immediately, returns bool
})
```

### Async Functions (Concurrent Execution)

* **Activities**: Run concurrently while in a state, automatically cancelled on state exit

```rust
// Async - returns a Future that runs concurrently
activity!(|_ctx, inst: &mut MyInstance, _event| {
    Box::pin(async move {
        // This runs concurrently with the state machine
        tokio::time::sleep(Duration::from_secs(1)).await;
        // Work continues...
    })
})
```

**Why this matters**: Entry/exit/guards/effects are part of the transition logic and must complete immediately. Activities represent ongoing work that happens *while* in a state.

## 🎓 Best Practices

### 1. Use Macros for Concise Definitions

```rust
// ✅ Good - using macros
let model = define!("Machine",
    initial!(target!("start")),
    state!("start", transition!(on!("next"), target!("../end"))),
    state!("end")
);

// ❌ Verbose - manual builder calls
let model = define(
    "Machine",
    vec![
        initial_with_target(target("start")),
        state_with_behaviors("start", vec![
            transition(vec![on("next"), target("../end")])
        ]),
        state("end")
    ]
);
```

### 2. Type Your Instance Properly

```rust
// ✅ Good - specific instance type
#[derive(Debug)]
struct GameInstance {
    score: i32,
    level: u32,
}

impl Instance for GameInstance {
    // Implementation
}

// ❌ Bad - generic catch-all
struct Instance {
    data: HashMap<String, Box<dyn Any>>,
}
```

### 3. Use Guards for Conditional Logic

```rust
// ✅ Good - guards determine transition
choice!("check",
    transition!(guard!(is_valid), target!("../success")),
    transition!(target!("../failure"))
)

// ❌ Bad - logic in effects
state!("check",
    effect!(|ctx, inst, event| {
        if is_valid(ctx, inst, event) {
            // Manually transition... not recommended
        }
        Box::pin(async move {})
    })
)
```

### 4. Always Validate Models

```rust
let model = define!(/* ... */);

// ✅ Always validate before starting
validate(&model)?;
let hsm = start(&ctx, instance, model)?;
```

## 📖 Documentation

Generate and view the full API documentation:

```bash
cargo doc --open
```

## 🤝 Contributing

Contributions are welcome! Please ensure:

1. All tests pass: `cargo test`
2. Code is formatted: `cargo fmt`
3. No clippy warnings: `cargo clippy`
4. Documentation is updated

## 📜 License

This project is licensed under the MIT License.

## 🔗 Related Implementations

Part of the [StateForward HSM](https://github.com/stateforward/hsm) multi-language project:

* **C++**: Template-based compile-time state machines
* **Go**: Concurrent state machines with OpenTelemetry
* **JavaScript/TypeScript**: High-performance web-ready implementation
* **Python**: asyncio-based implementation
* **Zig**: Systems programming state machines (WIP)

## 🙏 Acknowledgments

This implementation follows the HSM specification used across all StateForward implementations, ensuring consistent behavior and API design patterns across languages while leveraging Rust's unique strengths for safety and performance.

***

**Built with 🦀 Rust for memory safety and blazing fast performance!**
