-- The workspace list asks "which workspaces have unseen turns?" on every poll,
-- which scanned every row of coding_agent_turns joined three ways. Nearly all
-- rows are seen, so a partial index keeps only the interesting ones and stays
-- tiny as history grows.
CREATE INDEX IF NOT EXISTS idx_coding_agent_turns_unseen
    ON coding_agent_turns(seen)
    WHERE seen = 0;
