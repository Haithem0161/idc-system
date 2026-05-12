use async_trait::async_trait;
use uuid::Uuid;

use crate::db::Tx;
use crate::error::AppResult;

use super::entities::Patient;

#[async_trait]
pub trait PatientRepo: Send + Sync {
    async fn upsert(&self, tx: &mut Tx<'_>, p: &Patient) -> AppResult<()>;
    async fn get_by_id(&self, id: Uuid) -> AppResult<Option<Patient>>;
    async fn list_recent(&self, entity_id: &str, limit: i64) -> AppResult<Vec<Patient>>;
    async fn search(&self, entity_id: &str, query: &str, limit: i64) -> AppResult<Vec<Patient>>;
    async fn count_live_visits(&self, patient_id: Uuid) -> AppResult<i64>;
}
