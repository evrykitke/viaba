//! HR's side of the `CostCentres` port.
//!
//! Implemented here rather than in `app-hr`, which compiles to wasm and has no
//! database. Every port implementation will have this shape: rules in the app
//! crate, statements in `phonix-db`, the seam in a service.
//!
//! Ungated, like `master::tax::treatment_on` and for the same reason — it is
//! not a screen, it is what another app calls while posting, and that app has
//! already checked its own caller.

use phonix_db::hr::department as store;
use phonix_db::sqlx::PgPool;
use phonix_ports::cost_centre::{CostCentre, CostCentres, PORT};
use phonix_ports::error::PortError;
use uuid::Uuid;

/// The `CostCentres` port, over this workspace's departments. Owns its pool so
/// it can be handed over as a `dyn CostCentres` with no lifetime to thread.
#[derive(Clone)]
pub struct HrCostCentres {
    pool: PgPool,
}

impl HrCostCentres {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl CostCentres for HrCostCentres {
    async fn list(&self) -> Result<Vec<CostCentre>, PortError> {
        let departments = store::cost_centres(&self.pool)
            .await
            .map_err(|err| PortError::unavailable(PORT, err))?;

        // `as_cost_centre` is the one place a department becomes one, so the
        // query and it cannot disagree about what qualifies.
        Ok(departments
            .iter()
            .filter_map(|department| department.as_cost_centre())
            .collect())
    }

    async fn resolve(&self, id: Uuid) -> Result<Option<CostCentre>, PortError> {
        let Some(department) = store::find(&self.pool, id)
            .await
            .map_err(|err| PortError::unavailable(PORT, err))?
        else {
            return Ok(None);
        };

        Ok(department.as_cost_centre())
    }
}
