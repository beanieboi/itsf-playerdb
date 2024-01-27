use std::sync::{Arc, Mutex, Weak};

struct BackgroundOperationInner {
    progress: usize,
    max: usize,
    log: Vec<String>,
}

pub struct BackgroundOperationProgress {
    title: String,
    inner: Mutex<BackgroundOperationInner>,
}

impl BackgroundOperationProgress {
    pub fn set_progress(&self, progress: usize, max: usize) {
        let mut inner = self.inner.lock().expect("failed to lock mutex");
        inner.progress = progress;
        inner.max = max;
    }

    pub fn get_log(&self) -> Vec<String> {
        let inner = self.inner.lock().expect("failed to lock mutex");
        inner.log.clone()
    }

    pub fn log(&self, entry: String) {
        let mut inner = self.inner.lock().expect("failed to lock mutex");
        log::error!("{}", entry);
        inner.log.push(entry);
    }

    pub fn new(title: &str, max: usize) -> (Arc<BackgroundOperationProgress>, Weak<BackgroundOperationProgress>) {
        let this = BackgroundOperationProgress {
            title: title.into(),
            inner: Mutex::new(BackgroundOperationInner {
                progress: 0,
                max,
                log: Vec::new(),
            }),
        };
        let arc = Arc::new(this);
        let weak = Arc::downgrade(&arc);
        (arc, weak)
    }
}
