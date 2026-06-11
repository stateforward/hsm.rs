use rust::context::*;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_context_creation() {
    let ctx = Context::new();
    assert!(!ctx.is_done(), "New context should not be done");

    let ctx_default = Context::default();
    assert!(!ctx_default.is_done(), "Default context should not be done");
}

#[test]
fn test_context_cancellation() {
    let ctx = Context::new();
    assert!(!ctx.is_done());

    ctx.cancel();
    assert!(ctx.is_done(), "Context should be done after cancel");

    // Multiple cancels should be safe
    ctx.cancel();
    assert!(ctx.is_done());
}

#[test]
fn test_context_clone() {
    let ctx1 = Context::new();
    ctx1.cancel();

    // Clone should copy the current state
    let ctx2 = ctx1.clone();
    assert!(ctx2.is_done(), "Cloned context should preserve done state");

    // Clones share cancellation state so they can be used as cancellation tokens.
    let ctx3 = Context::new();
    let ctx4 = ctx3.clone();

    ctx3.cancel();
    assert!(ctx3.is_done());
    assert!(
        ctx4.is_done(),
        "Cloned context should share cancellation state"
    );
}

#[test]
fn test_context_thread_safety() {
    // Test that context can be safely shared across threads
    let ctx = Arc::new(Context::new());
    let ctx_clone = ctx.clone();

    let handle = thread::spawn(move || {
        // Check initial state in thread
        assert!(!ctx_clone.is_done());

        // Wait a bit
        thread::sleep(Duration::from_millis(50));

        // Cancel from thread
        ctx_clone.cancel();

        assert!(ctx_clone.is_done());
    });

    // Check in main thread
    assert!(!ctx.is_done());

    // Wait for other thread
    handle.join().unwrap();

    // Should see the cancellation from other thread
    assert!(ctx.is_done());
}

#[test]
fn test_context_atomic_ordering() {
    // Test multiple threads reading/writing concurrently
    let ctx = Arc::new(Context::new());
    let mut handles = vec![];

    // Spawn readers
    for _ in 0..5 {
        let ctx_clone = ctx.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..1000 {
                let _ = ctx_clone.is_done();
            }
        }));
    }

    // Spawn a canceller
    let ctx_clone = ctx.clone();
    handles.push(thread::spawn(move || {
        thread::sleep(Duration::from_millis(10));
        ctx_clone.cancel();
    }));

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    assert!(ctx.is_done());
}

#[test]
fn test_context_memory_ordering() {
    // Verify that Acquire/Release ordering works correctly
    use std::sync::atomic::{AtomicBool, Ordering};

    let ctx = Arc::new(Context::new());
    let data_written = Arc::new(AtomicBool::new(false));

    let ctx_clone = ctx.clone();
    let data_written_clone = data_written.clone();

    // Writer thread
    let writer = thread::spawn(move || {
        // Write some data
        data_written_clone.store(true, Ordering::Release);

        // Then cancel context
        ctx_clone.cancel();
    });

    // Reader thread
    let ctx_clone = ctx.clone();
    let data_written_clone = data_written.clone();

    let reader = thread::spawn(move || {
        // Wait for cancellation
        while !ctx_clone.is_done() {
            thread::yield_now();
        }

        // After seeing cancellation, we should see the data write
        assert!(
            data_written_clone.load(Ordering::Acquire),
            "Should see data write after context cancellation"
        );
    });

    writer.join().unwrap();
    reader.join().unwrap();
}

#[test]
fn test_context_use_patterns() {
    // Test common usage patterns

    // Pattern 1: Check cancellation in a loop
    let ctx = Context::new();
    let mut iterations = 0;

    while !ctx.is_done() && iterations < 5 {
        iterations += 1;
    }
    assert_eq!(iterations, 5);

    ctx.cancel();
    iterations = 0;

    while !ctx.is_done() && iterations < 5 {
        iterations += 1;
    }
    assert_eq!(
        iterations, 0,
        "Should exit immediately when context is done"
    );

    // Pattern 2: Early exit from function
    fn do_work(ctx: &Context) -> Result<i32, &'static str> {
        if ctx.is_done() {
            return Err("context cancelled");
        }

        // Simulate some work
        let result = 42;

        if ctx.is_done() {
            return Err("context cancelled");
        }

        Ok(result)
    }

    let ctx = Context::new();
    assert_eq!(do_work(&ctx), Ok(42));

    ctx.cancel();
    assert_eq!(do_work(&ctx), Err("context cancelled"));
}

#[test]
fn test_context_debug_impl() {
    let ctx = Context::new();
    let debug_str = format!("{:?}", ctx);
    assert!(
        debug_str.contains("Context"),
        "Debug should mention Context"
    );
    assert!(debug_str.contains("done"), "Debug should show done field");
}
