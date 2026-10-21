//! The cards `POST /api/cards` has filed, for its daily limit
//! (`api.cards.daily_limit`).
//!
//! The queries are checked at run time rather than through `sqlx::query!`:
//! they are two, they touch one table, and the route's tests exercise them
//! against a fresh database.

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct CardFiling;

impl CardFiling {
    /// Records one filed card.
    pub async fn record(
        pool: &SqlitePool,
        issue_id: Uuid,
        filed_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO card_filings (id, issue_id, filed_at) VALUES (?, ?, ?)")
            .bind(Uuid::new_v4())
            .bind(issue_id)
            .bind(filed_at)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// When each card filed after `since` was filed, oldest first.
    pub async fn filed_since(
        pool: &SqlitePool,
        since: DateTime<Utc>,
    ) -> Result<Vec<DateTime<Utc>>, sqlx::Error> {
        sqlx::query_scalar("SELECT filed_at FROM card_filings WHERE filed_at > ? ORDER BY filed_at")
            .bind(since)
            .fetch_all(pool)
            .await
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    #[tokio::test]
    async fn filings_are_counted_from_a_moment_oldest_first() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let now = Utc::now();
        for hours in [30, 2, 20] {
            CardFiling::record(&pool, Uuid::new_v4(), now - Duration::hours(hours))
                .await
                .unwrap();
        }
        let filed = CardFiling::filed_since(&pool, now - Duration::hours(24))
            .await
            .unwrap();
        assert_eq!(filed, [now - Duration::hours(20), now - Duration::hours(2)]);
    }
}
