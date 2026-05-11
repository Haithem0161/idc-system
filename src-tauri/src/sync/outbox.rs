//! Outbox helpers used by the push loop. The repository trait lives in
//! `domains::sync::domain::repositories::OutboxRepo`; this module only holds
//! transport-layer conversions.

use crate::domains::sync::domain::entities::OutboxOp;
use crate::domains::sync::infrastructure::{encode_payload, PushOp};

pub fn to_push_op(op: &OutboxOp) -> PushOp {
    PushOp {
        op_id: op.op_id.to_string(),
        entity: op.entity.clone(),
        entity_id: op.entity_id.clone(),
        op: op.op.as_str().to_string(),
        payload_b64: encode_payload(&op.payload),
    }
}
