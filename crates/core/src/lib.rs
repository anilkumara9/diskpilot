//! DiskPilot core engine.
//!
//! Pipeline: scanner (read) -> classifier (pure) -> recommendations
//! (explain only) -> planner (preview) -> safety gate (validate) ->
//! executor (act, record). Only the executor removes anything.

pub mod apps;
pub mod assistant;
pub mod classifier;
pub mod cleanup;
pub mod devtools;
pub mod disk;
pub mod docker;
pub mod duplicates;
pub mod error;
pub mod explorer;
pub mod history;
pub mod model;
pub mod privacy;
pub mod recommend;
pub mod safety;
pub mod scanner;
pub mod secrets;
pub mod snapshot;
pub mod state;

pub use error::{CoreError, CoreResult};
pub use model::*;
pub use state::AppState;
