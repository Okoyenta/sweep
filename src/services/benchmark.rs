use std::time::Instant;

use crate::domain::models::BenchmarkSample;
use crate::infra::paths::free_bytes_on_index_volume;

pub struct BenchmarkRecorder {
    before_free_bytes: u64,
    start: Instant,
    category_bytes: Vec<(String, u64)>,
}

impl BenchmarkRecorder {
    pub fn start() -> Self {
        Self {
            before_free_bytes: free_bytes_on_index_volume(),
            start: Instant::now(),
            category_bytes: Vec::new(),
        }
    }

    pub fn record_category(&mut self, id: &str, bytes: u64) {
        self.category_bytes.push((id.to_string(), bytes));
    }

    pub fn finish(self) -> BenchmarkSample {
        let after_free_bytes = free_bytes_on_index_volume();
        BenchmarkSample {
            before_free_bytes: self.before_free_bytes,
            after_free_bytes,
            elapsed_secs: self.start.elapsed().as_secs_f64(),
            category_bytes: self.category_bytes,
        }
    }
}
