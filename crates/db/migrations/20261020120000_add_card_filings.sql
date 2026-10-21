-- Every card filed through `POST /api/cards`, so the route can count the last
-- 24 hours against `api.cards.daily_limit`.
--
-- Counted here rather than from the cards' metadata: the board holds the
-- cards, and asking it would mean listing every board's cards on every filing;
-- and an incident tool's pickup gives its cards an origin too, without going
-- through the route. A card deleted from the board still counts for the day it
-- was filed. No foreign key: the card lives in the board's database.
CREATE TABLE card_filings (
    id        BLOB PRIMARY KEY NOT NULL,
    issue_id  BLOB NOT NULL,
    filed_at  DATETIME NOT NULL
);

CREATE INDEX idx_card_filings_filed_at ON card_filings (filed_at);
