// Error types for HSM library

use thiserror::Error;

/// HSM Error types
#[derive(Error, Debug)]
pub enum HsmError {
    #[error("Validation failed: {0}")]
    Validation(String),
    
    #[error("Runtime error: {0}")]
    Runtime(String),
    
    #[error("State machine error: {0}")]
    StateMachine(String),
    
    #[error("Path resolution error: {0}")]
    Path(String),
    
    #[error("Event processing error: {0}")]
    Event(String),
    
    #[error("Context error: {0}")]
    Context(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Channel error: {0}")]
    Channel(String),
}

/// Result type alias for HSM operations
pub type Result<T> = std::result::Result<T, HsmError>;