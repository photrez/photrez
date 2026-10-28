use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Cloneable best-effort cancellation signal, checked between print stages.
#[derive(Clone, Default)]
pub(crate) struct PrintCancellation(Arc<AtomicBool>);

impl PrintCancellation {
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::PrintCancellation;

    #[test]
    fn clones_share_cancellation_state() {
        let cancellation = PrintCancellation::default();
        let worker = cancellation.clone();
        assert!(!worker.is_cancelled());
        cancellation.cancel();
        assert!(worker.is_cancelled());
    }
}
