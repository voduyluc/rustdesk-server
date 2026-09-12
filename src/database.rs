use async_trait::async_trait;
use hbb_common::{log, ResultType};
use sqlx::{
    sqlite::SqliteConnectOptions, ConnectOptions, Connection, Error as SqlxError, SqliteConnection,
};
use std::{ops::DerefMut, str::FromStr};

#[derive(Default, sqlx::FromRow)]
pub struct WhitelistEntry {
    pub id: String,
    pub note: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Default, sqlx::FromRow)]
pub struct PeerListRecord {
    pub id: String,
    pub status: Option<i64>,
    pub note: Option<String>,
    pub created_at: Option<String>,
    pub last_online: Option<String>,
    pub info: String,
}
//use sqlx::postgres::PgPoolOptions;
//use sqlx::mysql::MySqlPoolOptions;

type Pool = deadpool::managed::Pool<DbPool>;

pub struct DbPool {
    url: String,
}

#[async_trait]
impl deadpool::managed::Manager for DbPool {
    type Type = SqliteConnection;
    type Error = SqlxError;
    async fn create(&self) -> Result<SqliteConnection, SqlxError> {
        let mut opt = SqliteConnectOptions::from_str(&self.url).unwrap();
        opt.log_statements(log::LevelFilter::Debug);
        SqliteConnection::connect_with(&opt).await
    }
    async fn recycle(
        &self,
        obj: &mut SqliteConnection,
    ) -> deadpool::managed::RecycleResult<SqlxError> {
        Ok(obj.ping().await?)
    }
}

#[derive(Clone)]
pub struct Database {
    pool: Pool,
}

#[derive(Default)]
pub struct Peer {
    pub guid: Vec<u8>,
    pub id: String,
    pub uuid: Vec<u8>,
    pub pk: Vec<u8>,
    pub user: Option<Vec<u8>>,
    pub info: String,
    pub status: Option<i64>,
}

impl Database {
    pub async fn new(url: &str) -> ResultType<Database> {
        if !std::path::Path::new(url).exists() {
            std::fs::File::create(url).ok();
        }
        let n: usize = std::env::var("MAX_DATABASE_CONNECTIONS")
            .unwrap_or_else(|_| "1".to_owned())
            .parse()
            .unwrap_or(1);
        log::debug!("MAX_DATABASE_CONNECTIONS={}", n);
        let pool = Pool::new(
            DbPool {
                url: url.to_owned(),
            },
            n,
        );
        let _ = pool.get().await?; // test
        let db = Database { pool };
        db.create_tables().await?;
        Ok(db)
    }

    async fn create_tables(&self) -> ResultType<()> {
        sqlx::query!(
            "
            create table if not exists peer (
                guid blob primary key not null,
                id varchar(100) not null,
                uuid blob not null,
                pk blob not null,
                created_at datetime not null default(current_timestamp),
                user blob,
                status tinyint,
                note varchar(300),
                info text not null
            ) without rowid;
            create unique index if not exists index_peer_id on peer (id);
            create index if not exists index_peer_user on peer (user);
            create index if not exists index_peer_created_at on peer (created_at);
            create index if not exists index_peer_status on peer (status);
        "
        )
        .execute(self.pool.get().await?.deref_mut())
        .await?;

        // Migrate: add last_online column if not already present
        let _ = sqlx::query("ALTER TABLE peer ADD COLUMN last_online DATETIME")
            .execute(self.pool.get().await?.deref_mut())
            .await;

        // Whitelist table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS whitelist (
                id VARCHAR(100) PRIMARY KEY NOT NULL,
                note VARCHAR(300),
                created_at DATETIME NOT NULL DEFAULT(current_timestamp)
            ) WITHOUT ROWID",
        )
        .execute(self.pool.get().await?.deref_mut())
        .await?;

        Ok(())
    }

    pub async fn get_peer(&self, id: &str) -> ResultType<Option<Peer>> {
        Ok(sqlx::query_as!(
            Peer,
            "select guid, id, uuid, pk, user, status, info from peer where id = ?",
            id
        )
        .fetch_optional(self.pool.get().await?.deref_mut())
        .await?)
    }

    pub async fn insert_peer(
        &self,
        id: &str,
        uuid: &[u8],
        pk: &[u8],
        info: &str,
    ) -> ResultType<Vec<u8>> {
        let guid = uuid::Uuid::new_v4().as_bytes().to_vec();
        sqlx::query!(
            "insert into peer(guid, id, uuid, pk, info) values(?, ?, ?, ?, ?)",
            guid,
            id,
            uuid,
            pk,
            info
        )
        .execute(self.pool.get().await?.deref_mut())
        .await?;
        Ok(guid)
    }

    pub async fn update_pk(
        &self,
        guid: &Vec<u8>,
        id: &str,
        pk: &[u8],
        info: &str,
    ) -> ResultType<()> {
        sqlx::query!(
            "update peer set id=?, pk=?, info=? where guid=?",
            id,
            pk,
            info,
            guid
        )
        .execute(self.pool.get().await?.deref_mut())
        .await?;
        Ok(())
    }

    pub async fn update_last_online(&self, guid: &[u8]) -> ResultType<()> {
        sqlx::query("UPDATE peer SET last_online = current_timestamp WHERE guid = ?")
            .bind(guid)
            .execute(self.pool.get().await?.deref_mut())
            .await?;
        Ok(())
    }

    pub async fn set_peer_status(&self, id: &str, status: i64) -> ResultType<()> {
        sqlx::query("UPDATE peer SET status = ? WHERE id = ?")
            .bind(status)
            .bind(id)
            .execute(self.pool.get().await?.deref_mut())
            .await?;
        Ok(())
    }

    pub async fn list_peers(&self) -> ResultType<Vec<PeerListRecord>> {
        Ok(sqlx::query_as::<_, PeerListRecord>(
            "SELECT id, status, note, created_at, last_online, info FROM peer ORDER BY created_at DESC",
        )
        .fetch_all(self.pool.get().await?.deref_mut())
        .await?)
    }

    pub async fn is_in_whitelist(&self, id: &str) -> ResultType<bool> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM whitelist WHERE id = ?")
                .bind(id)
                .fetch_one(self.pool.get().await?.deref_mut())
                .await?;
        Ok(count > 0)
    }

    pub async fn add_to_whitelist(&self, id: &str, note: &str) -> ResultType<()> {
        sqlx::query("INSERT OR REPLACE INTO whitelist(id, note) VALUES(?, ?)")
            .bind(id)
            .bind(note)
            .execute(self.pool.get().await?.deref_mut())
            .await?;
        Ok(())
    }

    pub async fn remove_from_whitelist(&self, id: &str) -> ResultType<()> {
        sqlx::query("DELETE FROM whitelist WHERE id = ?")
            .bind(id)
            .execute(self.pool.get().await?.deref_mut())
            .await?;
        Ok(())
    }

    pub async fn list_whitelist(&self) -> ResultType<Vec<WhitelistEntry>> {
        Ok(sqlx::query_as::<_, WhitelistEntry>(
            "SELECT id, note, created_at FROM whitelist ORDER BY created_at DESC",
        )
        .fetch_all(self.pool.get().await?.deref_mut())
        .await?)
    }
}

#[cfg(test)]
mod tests {
    use hbb_common::tokio;
    #[test]
    fn test_insert() {
        insert();
    }

    #[tokio::main(flavor = "multi_thread")]
    async fn insert() {
        let db = super::Database::new("test.sqlite3").await.unwrap();
        let mut jobs = vec![];
        for i in 0..10000 {
            let cloned = db.clone();
            let id = i.to_string();
            let a = tokio::spawn(async move {
                let empty_vec = Vec::new();
                cloned
                    .insert_peer(&id, &empty_vec, &empty_vec, "")
                    .await
                    .unwrap();
            });
            jobs.push(a);
        }
        for i in 0..10000 {
            let cloned = db.clone();
            let id = i.to_string();
            let a = tokio::spawn(async move {
                cloned.get_peer(&id).await.unwrap();
            });
            jobs.push(a);
        }
        hbb_common::futures::future::join_all(jobs).await;
    }
}
