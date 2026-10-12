-- Turns that stopped because the Claude seat hit its usage limit, one row per
-- execution process. The seat pause itself is process-wide state that a
-- restart clears; which sessions it stranded is not recoverable that way, and
-- a restart is exactly when someone wants the board walked back, so it is
-- recorded here instead.
--
-- A separate table (not columns on execution_processes) so the existing
-- queries and the generated ExecutionProcess type stay untouched.
--
-- resumed_at is stamped rather than the row deleted: it is what makes a second
-- resume of the same stall impossible, and it keeps the record of how long a
-- window kept the board waiting.
CREATE TABLE execution_process_seat_stall (
    execution_process_id BLOB PRIMARY KEY NOT NULL
        REFERENCES execution_processes(id) ON DELETE CASCADE,
    detected_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resumed_at DATETIME
);

-- The resume sweep asks for the oldest unresumed stall on every tick.
CREATE INDEX idx_seat_stall_pending
    ON execution_process_seat_stall (detected_at)
    WHERE resumed_at IS NULL;
