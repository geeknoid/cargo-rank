use std::io::Write;

/// Abstract the host environment to enable testing
pub trait Host: Send + Sync {
    // where to send normal output (e.g., stdout)
    fn output(&mut self) -> impl Write;

    // where to send error output (e.g., stderr)
    fn error(&mut self) -> impl Write;

    /// Terminate the process (although in a test environment this might just set a flag and return).
    fn exit(&mut self, code: i32);
}
