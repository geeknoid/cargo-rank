/// A trait for reporting progress of long-running operations.
pub trait Progress: Send + Sync {
    /// Set the phase label for the current operation (e.g., "Preparing", "Collecting").
    fn set_phase(&self, phase: &str);

    /// Configure determinate progress reporting.
    ///
    /// The callback should return (total, current, message) to show progress
    /// as a percentage or fraction.
    fn set_determinate(&self, callback: Box<dyn Fn() -> (u64, u64, String) + Send + Sync + 'static>);

    /// Configure indeterminate progress reporting.
    ///
    /// The callback should return a message string. Use this for operations
    /// where the total amount of work is unknown.
    fn set_indeterminate(&self, callback: Box<dyn Fn() -> String + Send + Sync + 'static>);

    /// Finish and clear the progress indicator.
    fn done(&self);
}
