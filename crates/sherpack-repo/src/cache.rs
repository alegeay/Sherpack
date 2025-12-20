//! SQLite-based cache with FTS5 for fast search
//!
//! Features:
//! - WAL mode for better concurrency
//! - FTS5 full-text search
//! - Auto-recovery on corruption

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags, params};
use std::path::{Path, PathBuf};

use crate::error::{RepoError, Result};
use crate::index::PackEntry;

/// SQLite cache for repository indices
pub struct IndexCache {
    conn: Connection,
}

impl IndexCache {
    /// Open or create cache at default location
    pub fn open() -> Result<Self> {
        let path = Self::default_path()?;
        Self::open_at(&path)
    }

    /// Open or create cache at specific path
    pub fn open_at(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Try to open existing database
        let result = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        );

        let conn = match result {
            Ok(conn) => conn,
            Err(e) => {
                // If corrupted, delete and recreate
                tracing::warn!("Cache corrupted, recreating: {}", e);
                if path.exists() {
                    std::fs::remove_file(path)?;
                }
                Connection::open(path)?
            }
        };

        let mut cache = Self { conn };
        cache.init()?;
        Ok(cache)
    }

    /// Open in-memory cache (for testing)
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let mut cache = Self { conn };
        cache.init()?;
        Ok(cache)
    }

    /// Get default cache path
    pub fn default_path() -> Result<PathBuf> {
        let cache_dir = dirs::cache_dir().ok_or_else(|| RepoError::CacheError {
            message: "Could not determine cache directory".to_string(),
        })?;
        Ok(cache_dir.join("sherpack").join("index.db"))
    }

    /// Initialize database schema
    fn init(&mut self) -> Result<()> {
        // Enable WAL mode for better concurrency
        self.conn.pragma_update(None, "journal_mode", "WAL")?;
        self.conn.pragma_update(None, "synchronous", "NORMAL")?;

        self.conn.execute_batch(
            r#"
            -- Repositories table
            CREATE TABLE IF NOT EXISTS repositories (
                id INTEGER PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                url TEXT NOT NULL,
                repo_type TEXT NOT NULL,
                etag TEXT,
                last_updated INTEGER,
                pack_count INTEGER DEFAULT 0
            );

            -- Packs table
            CREATE TABLE IF NOT EXISTS packs (
                id INTEGER PRIMARY KEY,
                repo_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                app_version TEXT,
                description TEXT,
                keywords TEXT,
                deprecated INTEGER DEFAULT 0,
                created INTEGER,
                digest TEXT,
                download_url TEXT,
                UNIQUE(repo_id, name, version)
            );

            -- FTS5 virtual table for full-text search
            CREATE VIRTUAL TABLE IF NOT EXISTS packs_fts USING fts5(
                name,
                description,
                keywords,
                content='packs',
                content_rowid='id'
            );

            -- Triggers to keep FTS in sync
            CREATE TRIGGER IF NOT EXISTS packs_ai AFTER INSERT ON packs BEGIN
                INSERT INTO packs_fts(rowid, name, description, keywords)
                VALUES (new.id, new.name, new.description, new.keywords);
            END;

            CREATE TRIGGER IF NOT EXISTS packs_ad AFTER DELETE ON packs BEGIN
                INSERT INTO packs_fts(packs_fts, rowid, name, description, keywords)
                VALUES ('delete', old.id, old.name, old.description, old.keywords);
            END;

            CREATE TRIGGER IF NOT EXISTS packs_au AFTER UPDATE ON packs BEGIN
                INSERT INTO packs_fts(packs_fts, rowid, name, description, keywords)
                VALUES ('delete', old.id, old.name, old.description, old.keywords);
                INSERT INTO packs_fts(rowid, name, description, keywords)
                VALUES (new.id, new.name, new.description, new.keywords);
            END;

            -- Indexes
            CREATE INDEX IF NOT EXISTS idx_packs_repo ON packs(repo_id);
            CREATE INDEX IF NOT EXISTS idx_packs_name ON packs(name);
            CREATE INDEX IF NOT EXISTS idx_packs_name_version ON packs(name, version);
            "#,
        )?;

        Ok(())
    }

    /// Add or update a repository
    pub fn upsert_repository(
        &mut self,
        name: &str,
        url: &str,
        repo_type: &str,
        etag: Option<&str>,
    ) -> Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO repositories (name, url, repo_type, etag, last_updated)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(name) DO UPDATE SET
                url = excluded.url,
                repo_type = excluded.repo_type,
                etag = excluded.etag,
                last_updated = excluded.last_updated
            "#,
            params![name, url, repo_type, etag, Utc::now().timestamp()],
        )?;

        let id = self.conn.last_insert_rowid();
        Ok(id)
    }

    /// Get repository ID by name
    pub fn get_repository_id(&self, name: &str) -> Result<Option<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM repositories WHERE name = ?1")?;
        let id = stmt.query_row([name], |row| row.get(0)).ok();
        Ok(id)
    }

    /// Remove a repository and its packs
    pub fn remove_repository(&mut self, name: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM repositories WHERE name = ?1", [name])?;
        Ok(())
    }

    /// Add packs to cache for a repository
    pub fn add_packs(&mut self, repo_name: &str, packs: &[PackEntry]) -> Result<()> {
        let repo_id =
            self.get_repository_id(repo_name)?
                .ok_or_else(|| RepoError::RepositoryNotFound {
                    name: repo_name.to_string(),
                })?;

        // Use a transaction for better performance
        let tx = self.conn.transaction()?;

        // Clear existing packs for this repo
        tx.execute("DELETE FROM packs WHERE repo_id = ?1", [repo_id])?;

        // Insert new packs
        let mut stmt = tx.prepare(
            r#"
            INSERT INTO packs (repo_id, name, version, app_version, description, keywords, deprecated, created, digest, download_url)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
        )?;

        for pack in packs {
            let keywords = pack.keywords.join(",");
            let created = pack.created.map(|d| d.timestamp());
            let download_url = pack.download_url();

            stmt.execute(params![
                repo_id,
                pack.name,
                pack.version,
                pack.app_version,
                pack.description,
                keywords,
                pack.deprecated as i32,
                created,
                pack.digest,
                download_url,
            ])?;
        }
        drop(stmt);

        // Update pack count
        tx.execute(
            "UPDATE repositories SET pack_count = ?1 WHERE id = ?2",
            params![packs.len() as i64, repo_id],
        )?;

        tx.commit()?;
        Ok(())
    }

    /// Full-text search across all repositories
    pub fn search(&self, query: &str) -> Result<Vec<CachedPack>> {
        // FTS5 query with ranking
        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, r.name, p.name, p.version, p.app_version, p.description,
                   p.keywords, p.deprecated, p.digest, p.download_url,
                   bm25(packs_fts) as rank
            FROM packs_fts fts
            JOIN packs p ON p.id = fts.rowid
            JOIN repositories r ON r.id = p.repo_id
            WHERE packs_fts MATCH ?1
            ORDER BY rank
            LIMIT 100
            "#,
        )?;

        let packs = stmt
            .query_map([query], |row| {
                Ok(CachedPack {
                    id: row.get(0)?,
                    repo_name: row.get(1)?,
                    name: row.get(2)?,
                    version: row.get(3)?,
                    app_version: row.get(4)?,
                    description: row.get(5)?,
                    keywords: row
                        .get::<_, Option<String>>(6)?
                        .map(|k| k.split(',').map(String::from).collect())
                        .unwrap_or_default(),
                    deprecated: row.get::<_, i32>(7)? != 0,
                    digest: row.get(8)?,
                    download_url: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(packs)
    }

    /// Search within a specific repository
    pub fn search_in_repo(&self, repo_name: &str, query: &str) -> Result<Vec<CachedPack>> {
        let repo_id =
            self.get_repository_id(repo_name)?
                .ok_or_else(|| RepoError::RepositoryNotFound {
                    name: repo_name.to_string(),
                })?;

        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, r.name, p.name, p.version, p.app_version, p.description,
                   p.keywords, p.deprecated, p.digest, p.download_url
            FROM packs_fts fts
            JOIN packs p ON p.id = fts.rowid
            JOIN repositories r ON r.id = p.repo_id
            WHERE packs_fts MATCH ?1 AND p.repo_id = ?2
            ORDER BY bm25(packs_fts)
            LIMIT 100
            "#,
        )?;

        let packs = stmt
            .query_map(params![query, repo_id], |row| {
                Ok(CachedPack {
                    id: row.get(0)?,
                    repo_name: row.get(1)?,
                    name: row.get(2)?,
                    version: row.get(3)?,
                    app_version: row.get(4)?,
                    description: row.get(5)?,
                    keywords: row
                        .get::<_, Option<String>>(6)?
                        .map(|k| k.split(',').map(String::from).collect())
                        .unwrap_or_default(),
                    deprecated: row.get::<_, i32>(7)? != 0,
                    digest: row.get(8)?,
                    download_url: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(packs)
    }

    /// Get latest version of each pack in a repository
    pub fn list_latest(&self, repo_name: &str) -> Result<Vec<CachedPack>> {
        let repo_id =
            self.get_repository_id(repo_name)?
                .ok_or_else(|| RepoError::RepositoryNotFound {
                    name: repo_name.to_string(),
                })?;

        // Get latest version of each pack
        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, r.name, p.name, p.version, p.app_version, p.description,
                   p.keywords, p.deprecated, p.digest, p.download_url
            FROM packs p
            JOIN repositories r ON r.id = p.repo_id
            WHERE p.repo_id = ?1
            AND p.version = (
                SELECT MAX(p2.version) FROM packs p2
                WHERE p2.repo_id = p.repo_id AND p2.name = p.name
            )
            ORDER BY p.name
            "#,
        )?;

        let packs = stmt
            .query_map([repo_id], |row| {
                Ok(CachedPack {
                    id: row.get(0)?,
                    repo_name: row.get(1)?,
                    name: row.get(2)?,
                    version: row.get(3)?,
                    app_version: row.get(4)?,
                    description: row.get(5)?,
                    keywords: row
                        .get::<_, Option<String>>(6)?
                        .map(|k| k.split(',').map(String::from).collect())
                        .unwrap_or_default(),
                    deprecated: row.get::<_, i32>(7)? != 0,
                    digest: row.get(8)?,
                    download_url: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(packs)
    }

    /// Get all versions of a pack
    pub fn get_pack_versions(&self, repo_name: &str, pack_name: &str) -> Result<Vec<CachedPack>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, r.name, p.name, p.version, p.app_version, p.description,
                   p.keywords, p.deprecated, p.digest, p.download_url
            FROM packs p
            JOIN repositories r ON r.id = p.repo_id
            WHERE r.name = ?1 AND p.name = ?2
            ORDER BY p.version DESC
            "#,
        )?;

        let packs = stmt
            .query_map(params![repo_name, pack_name], |row| {
                Ok(CachedPack {
                    id: row.get(0)?,
                    repo_name: row.get(1)?,
                    name: row.get(2)?,
                    version: row.get(3)?,
                    app_version: row.get(4)?,
                    description: row.get(5)?,
                    keywords: row
                        .get::<_, Option<String>>(6)?
                        .map(|k| k.split(',').map(String::from).collect())
                        .unwrap_or_default(),
                    deprecated: row.get::<_, i32>(7)? != 0,
                    digest: row.get(8)?,
                    download_url: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(packs)
    }

    /// Get cache statistics
    pub fn stats(&self) -> Result<CacheStats> {
        let repo_count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM repositories", [], |r| r.get(0))?;

        let pack_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM packs", [], |r| r.get(0))?;

        let oldest_update: Option<i64> = self
            .conn
            .query_row("SELECT MIN(last_updated) FROM repositories", [], |r| {
                r.get(0)
            })
            .ok();

        Ok(CacheStats {
            repository_count: repo_count as usize,
            pack_count: pack_count as usize,
            oldest_update: oldest_update
                .map(|ts| DateTime::from_timestamp(ts, 0).unwrap_or_default()),
        })
    }

    /// Clear all cache data
    pub fn clear(&mut self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            DELETE FROM packs;
            DELETE FROM repositories;
            "#,
        )?;
        Ok(())
    }

    /// Rebuild FTS index (for recovery)
    pub fn rebuild_fts(&mut self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            INSERT INTO packs_fts(packs_fts) VALUES('rebuild');
            "#,
        )?;
        Ok(())
    }
}

/// Cached pack information
#[derive(Debug, Clone)]
pub struct CachedPack {
    pub id: i64,
    pub repo_name: String,
    pub name: String,
    pub version: String,
    pub app_version: Option<String>,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub deprecated: bool,
    pub digest: Option<String>,
    pub download_url: Option<String>,
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub repository_count: usize,
    pub pack_count: usize,
    pub oldest_update: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_packs() -> Vec<PackEntry> {
        vec![
            PackEntry {
                name: "nginx".to_string(),
                version: "15.0.0".to_string(),
                app_version: Some("1.25.0".to_string()),
                description: Some("NGINX Open Source web server".to_string()),
                keywords: vec!["webserver".to_string(), "http".to_string()],
                urls: vec!["https://example.com/nginx-15.0.0.tgz".to_string()],
                ..Default::default()
            },
            PackEntry {
                name: "redis".to_string(),
                version: "17.0.0".to_string(),
                description: Some("Redis in-memory database".to_string()),
                keywords: vec!["cache".to_string(), "database".to_string()],
                urls: vec!["https://example.com/redis-17.0.0.tgz".to_string()],
                ..Default::default()
            },
        ]
    }

    #[test]
    fn test_cache_init() {
        let cache = IndexCache::open_memory().unwrap();
        let stats = cache.stats().unwrap();
        assert_eq!(stats.repository_count, 0);
        assert_eq!(stats.pack_count, 0);
    }

    #[test]
    fn test_add_repository() {
        let mut cache = IndexCache::open_memory().unwrap();
        let id = cache
            .upsert_repository("bitnami", "https://charts.bitnami.com", "http", None)
            .unwrap();
        assert!(id > 0);

        let repo_id = cache.get_repository_id("bitnami").unwrap();
        assert!(repo_id.is_some());
    }

    #[test]
    fn test_add_packs() {
        let mut cache = IndexCache::open_memory().unwrap();
        cache
            .upsert_repository("bitnami", "https://charts.bitnami.com", "http", None)
            .unwrap();

        let packs = sample_packs();
        cache.add_packs("bitnami", &packs).unwrap();

        let stats = cache.stats().unwrap();
        assert_eq!(stats.pack_count, 2);
    }

    #[test]
    fn test_search() {
        let mut cache = IndexCache::open_memory().unwrap();
        cache
            .upsert_repository("bitnami", "https://charts.bitnami.com", "http", None)
            .unwrap();
        cache.add_packs("bitnami", &sample_packs()).unwrap();

        // Search by name
        let results = cache.search("nginx").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "nginx");

        // Search by keyword
        let results = cache.search("cache").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "redis");

        // Search by description
        let results = cache.search("database").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_list_latest() {
        let mut cache = IndexCache::open_memory().unwrap();
        cache
            .upsert_repository("bitnami", "https://charts.bitnami.com", "http", None)
            .unwrap();
        cache.add_packs("bitnami", &sample_packs()).unwrap();

        let latest = cache.list_latest("bitnami").unwrap();
        assert_eq!(latest.len(), 2);
    }

    #[test]
    fn test_remove_repository() {
        let mut cache = IndexCache::open_memory().unwrap();
        cache
            .upsert_repository("bitnami", "https://charts.bitnami.com", "http", None)
            .unwrap();
        cache.add_packs("bitnami", &sample_packs()).unwrap();

        cache.remove_repository("bitnami").unwrap();

        let stats = cache.stats().unwrap();
        assert_eq!(stats.repository_count, 0);
        assert_eq!(stats.pack_count, 0);
    }
}
