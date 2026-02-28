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

/// Test host that captures output to in-memory buffers
#[cfg(test)]
pub struct TestHost {
    pub output_buf: Vec<u8>,
    pub error_buf: Vec<u8>,
}

#[cfg(test)]
impl TestHost {
    pub fn new() -> Self {
        Self {
            output_buf: Vec::new(),
            error_buf: Vec::new(),
        }
    }
}

#[cfg(test)]
impl Host for TestHost {
    fn output(&mut self) -> impl Write {
        std::io::Cursor::new(&mut self.output_buf)
    }

    fn error(&mut self) -> impl Write {
        std::io::Cursor::new(&mut self.error_buf)
    }

    fn exit(&mut self, _code: i32) {
        // In tests, don't actually exit
    }
}
