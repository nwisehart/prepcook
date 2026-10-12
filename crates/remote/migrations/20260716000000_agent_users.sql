-- Agent users: non-SSO identities for automation runners. Admins create
-- them with a server-generated secret; the local login path authenticates
-- them against agent_password_hash (SHA-256 hex of the generated secret —
-- acceptable because the secret is a random 256-bit token, never a
-- human-chosen password).
ALTER TABLE users
    ADD COLUMN is_agent BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN agent_password_hash TEXT;

CREATE INDEX idx_users_is_agent ON users(is_agent) WHERE is_agent;
