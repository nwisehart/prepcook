-- Per-turn token-usage ledger (BUG-9777). One row per execution process
-- (one process = one agent turn), written when the process exits from the
-- billing split the executor parsed out of the turn's final result message.
-- A separate table (not columns on execution_processes) so the existing
-- queries and the generated ExecutionProcess type stay untouched.
CREATE TABLE execution_process_usage (
    execution_process_id BLOB PRIMARY KEY NOT NULL
        REFERENCES execution_processes(id) ON DELETE CASCADE,
    input_tokens BIGINT,
    output_tokens BIGINT,
    cache_read_tokens BIGINT,
    cache_creation_tokens BIGINT,
    model TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
