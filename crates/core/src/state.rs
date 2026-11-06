//! Engine state shared by the shell commands: last scan, its index,
//! recommendations and outstanding cleanup plans.

use crate::cleanup::Planner;
use crate::model::{Recommendation, ScanSummary};
use crate::scanner::DirIndex;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Default)]
pub struct AppState {
    pub summary: Option<ScanSummary>,
    pub index: Option<DirIndex>,
    pub recommendations: Vec<Recommendation>,
    pub planner: Planner,
    pub scan_cancel: Arc<AtomicBool>,
    pub scanning: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn store_scan(&mut self, summary: ScanSummary, index: DirIndex) {
        self.recommendations = crate::recommend::build_recommendations(&summary, &index);
        self.summary = Some(summary);
        self.index = Some(index);
        self.scanning = false;
    }

    pub fn clear_scan(&mut self) {
        self.summary = None;
        self.index = None;
        self.recommendations = Vec::new();
    }
}
