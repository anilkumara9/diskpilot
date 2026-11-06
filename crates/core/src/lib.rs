//! DiskPilot core engine.
//!
//! Pipeline: scanner (read) -> classifier (pure) -> recommendations
//! (explain only) -> planner (preview) -> safety gate (validate) ->
//! executor (act, record). Only the executor removes anything.

pub mod apps;
pub mod assistant;
pub mod cards;
pub mod classifier;
pub mod cleanup;
pub mod cloud;
pub mod devtools;
pub mod disk;
pub mod docker;
pub mod drives;
pub mod duplicates;
pub mod error;
pub mod explorer;
pub mod forecast;
pub mod history;
pub mod media_meta;
pub mod model;
pub mod privacy;
pub mod provenance;
pub mod recommend;
pub mod safety;
pub mod scanner;
pub mod search;
pub mod secrets;
pub mod snapshot;
pub mod snapshot_diff;
pub mod state;

pub use error::{CoreError, CoreResult};
pub use model::*;
pub use state::AppState;
