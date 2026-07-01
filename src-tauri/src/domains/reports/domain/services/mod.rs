//! Pure domain helpers for the reports bounded context.

pub mod csv_writer;
pub mod input_hash;
pub mod money_trend;
pub mod tz;

pub use csv_writer::{
    write_doctor_earnings_csv, write_mandoub_earnings_csv, write_operator_earnings_csv,
    write_visits_csv,
};
pub use input_hash::compute_input_hash;
pub use money_trend::{permille_change, trend_cell, TrendInputs};
pub use tz::{baghdad_offset_seconds, local_day_utc_range, utc_to_local_date};
