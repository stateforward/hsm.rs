// Context for HSM execution (cancellation tokens, etc.)

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ContextKey {
    HSM,
    Owner,
    Instances,
}

#[allow(non_snake_case, non_upper_case_globals)]
pub mod Keys {
    pub const HSM: super::ContextKey = super::ContextKey::HSM;
    pub const Owner: super::ContextKey = super::ContextKey::Owner;
    pub const Instances: super::ContextKey = super::ContextKey::Instances;
}

pub struct Context {
    cancelled: Arc<AtomicBool>,
    deadline: Option<Instant>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: None,
        }
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        let ctx = Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: Some(Instant::now() + timeout),
        };

        let cancelled = ctx.cancelled.clone();
        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            cancelled.store(true, Ordering::Release);
        });

        ctx
    }

    pub fn is_cancelled(&self) -> bool {
        // Check atomic flag first
        if self.cancelled.load(Ordering::Acquire) {
            return true;
        }

        // Check deadline if set
        if let Some(deadline) = self.deadline {
            if Instant::now() >= deadline {
                self.cancelled.store(true, Ordering::Release);
                return true;
            }
        }

        false
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub(crate) fn registry_key(&self) -> usize {
        Arc::as_ptr(&self.cancelled) as usize
    }

    // Legacy compatibility
    pub fn is_done(&self) -> bool {
        self.is_cancelled()
    }
}

impl Clone for Context {
    fn clone(&self) -> Self {
        // Share the same cancellation state
        Self {
            cancelled: self.cancelled.clone(),
            deadline: self.deadline,
        }
    }
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("done", &self.is_cancelled())
            .field("deadline", &self.deadline)
            .finish()
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

pub fn make_context() -> Context {
    Context::new()
}

#[allow(non_snake_case)]
pub fn MakeContext() -> Context {
    make_context()
}
