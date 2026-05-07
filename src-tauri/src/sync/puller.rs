//! Pull loop: GET /sync/pull?since=<cursor> -> apply changes in a transaction
//! per row (compare `version` and `updated_at`) -> persist new cursor.
