-- Every human comment left on a merge request the orchestrator opened.
--
-- The orchestrator already fetches these every five minutes and injects them
-- into the implementing agent, then throws the content away and keeps a
-- timestamp. That makes the most important question about this system
-- unanswerable: which kind of feedback reviewers send back most often. Nobody
-- can say without reading threads by hand, and once a capability ships that
-- claims to reduce rejections, nobody can show that it did.
--
-- No foreign key, deliberately. The obvious parent is pull_requests, whose rows
-- go away with their workspace — and a record that disappears when the work
-- finishes is useless for exactly the longitudinal question this exists to
-- answer. The ids are stored as plain values and the table outlives everything
-- it refers to.
--
-- Bot comments are recorded too, flagged rather than dropped. Filtering at
-- collection is a decision that cannot be undone later, and the agent's own
-- triage notes are what make a thread readable in reconstruction.
CREATE TABLE merge_request_reviews (
    id           BLOB PRIMARY KEY NOT NULL,
    project_id   BLOB NOT NULL,
    issue_id     BLOB NOT NULL,
    -- Denormalised so a row still identifies its ticket after the card is gone.
    jira_key     TEXT,
    mr_url       TEXT NOT NULL,
    mr_number    INTEGER NOT NULL,
    -- 'general' (whole-MR note) or 'review' (anchored to a line).
    comment_kind TEXT NOT NULL,
    -- The host's own note id, as text: GitLab types the two kinds differently.
    comment_id   TEXT NOT NULL,
    author       TEXT NOT NULL,
    is_bot       BOOLEAN NOT NULL DEFAULT 0,
    body         TEXT NOT NULL,
    -- Set for anchored comments only. Where a reviewer points is often more
    -- telling than what they say.
    path         TEXT,
    line         INTEGER,
    commented_at DATETIME NOT NULL,
    recorded_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- The host's note id is the natural key, which is what makes recording
-- idempotent: comments are written before they are injected, and a failed
-- injection deliberately leaves the high-water mark alone so the same comments
-- arrive again on the next tick.
CREATE UNIQUE INDEX idx_mr_reviews_natural
    ON merge_request_reviews (project_id, mr_number, comment_kind, comment_id);

-- "Every comment on this ticket, oldest first" is the read the analysis wants.
CREATE INDEX idx_mr_reviews_issue
    ON merge_request_reviews (issue_id, commented_at);
