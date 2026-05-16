use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

use crate::device::manager::{Device, DeviceEndpoint};
use crate::message::manager::ChatMessage;
use crate::transfer::manager::FileTransfer;
use crate::transport::TransportPing;

#[derive(Clone, Debug)]
pub struct SqliteStore {
    db_path: PathBuf,
}

impl SqliteStore {
    pub fn open(db_path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create data directory failed: {err}"))?;
        }

        let store = Self { db_path };
        store.migrate()?;
        Ok(store)
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn list_devices(&self) -> Result<Vec<Device>, String> {
        let conn = self.connection()?;
        let mut statement = conn
            .prepare(
                "SELECT id, name, ip, port, public_key, trusted, platform, version,
                        protocol_version, features, online, created_at, last_seen_at,
                        last_checked_at, last_error
                 FROM devices
                 ORDER BY online DESC, COALESCE(last_seen_at, created_at) DESC, name COLLATE NOCASE",
            )
            .map_err(|err| format!("prepare list_devices failed: {err}"))?;

        let rows = statement
            .query_map([], |row| {
                let features_json: Option<String> = row.get(9)?;
                let features = features_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
                    .unwrap_or_default();
                let port: i64 = row.get(3)?;

                Ok(Device {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    ip: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    port: u16::try_from(port).unwrap_or(8765),
                    public_key: row.get(4)?,
                    trusted: row.get::<_, i64>(5)? != 0,
                    platform: row.get(6)?,
                    version: row.get(7)?,
                    protocol_version: row.get(8)?,
                    features,
                    online: row.get::<_, i64>(10)? != 0,
                    created_at: row.get(11)?,
                    last_seen_at: row.get(12)?,
                    last_checked_at: row.get(13)?,
                    last_error: row.get(14)?,
                })
            })
            .map_err(|err| format!("query list_devices failed: {err}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("read device row failed: {err}"))
    }

    pub fn upsert_online_device(
        &self,
        endpoint: &DeviceEndpoint,
        ping: &TransportPing,
        now: i64,
    ) -> Result<Device, String> {
        let conn = self.connection()?;
        let features_json = serde_json::to_string(&ping.features)
            .map_err(|err| format!("serialize device features failed: {err}"))?;

        conn.execute(
            "DELETE FROM devices WHERE (id = ?1 OR (ip = ?2 AND port = ?3)) AND id != ?1",
            params![&ping.device_id, &endpoint.host, i64::from(endpoint.port)],
        )
        .map_err(|err| format!("deduplicate device failed: {err}"))?;

        conn.execute(
            "INSERT INTO devices (
                id, name, ip, port, public_key, trusted, platform, version,
                protocol_version, features, online, created_at, last_seen_at,
                last_checked_at, last_error
             )
             VALUES (?1, ?2, ?3, ?4, NULL, 1, ?5, ?6, ?7, ?8, 1, ?9, ?9, ?9, NULL)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                ip = excluded.ip,
                port = excluded.port,
                trusted = 1,
                platform = excluded.platform,
                version = excluded.version,
                protocol_version = excluded.protocol_version,
                features = excluded.features,
                online = 1,
                last_seen_at = excluded.last_seen_at,
                last_checked_at = excluded.last_checked_at,
                last_error = NULL",
            params![
                &ping.device_id,
                &ping.device_name,
                &endpoint.host,
                i64::from(endpoint.port),
                &ping.platform,
                &ping.version,
                i64::from(ping.protocol_version),
                features_json,
                now,
            ],
        )
        .map_err(|err| format!("save device failed: {err}"))?;

        self.get_device(&ping.device_id)?
            .ok_or_else(|| "device was saved but could not be reloaded".to_string())
    }

    pub fn mark_device_offline(
        &self,
        device_id: &str,
        checked_at: i64,
        error: String,
    ) -> Result<(), String> {
        let conn = self.connection()?;
        conn.execute(
            "UPDATE devices
             SET online = 0, last_checked_at = ?2, last_error = ?3
             WHERE id = ?1",
            params![device_id, checked_at, error],
        )
        .map_err(|err| format!("mark device offline failed: {err}"))?;
        Ok(())
    }

    pub fn get_device(&self, device_id: &str) -> Result<Option<Device>, String> {
        let conn = self.connection()?;
        conn.query_row(
            "SELECT id, name, ip, port, public_key, trusted, platform, version,
                    protocol_version, features, online, created_at, last_seen_at,
                    last_checked_at, last_error
             FROM devices
             WHERE id = ?1",
            params![device_id],
            |row| {
                let features_json: Option<String> = row.get(9)?;
                let features = features_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
                    .unwrap_or_default();
                let port: i64 = row.get(3)?;

                Ok(Device {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    ip: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    port: u16::try_from(port).unwrap_or(8765),
                    public_key: row.get(4)?,
                    trusted: row.get::<_, i64>(5)? != 0,
                    platform: row.get(6)?,
                    version: row.get(7)?,
                    protocol_version: row.get(8)?,
                    features,
                    online: row.get::<_, i64>(10)? != 0,
                    created_at: row.get(11)?,
                    last_seen_at: row.get(12)?,
                    last_checked_at: row.get(13)?,
                    last_error: row.get(14)?,
                })
            },
        )
        .optional()
        .map_err(|err| format!("get device failed: {err}"))
    }

    pub fn list_messages(&self, peer_device_id: &str) -> Result<Vec<ChatMessage>, String> {
        let conn = self.connection()?;
        let mut statement = conn
            .prepare(
                "SELECT id, peer_device_id, peer_device_name, direction, content, status,
                        content_size, chunk_size, total_chunks, chunks_done,
                        created_at, updated_at, error
                 FROM messages
                 WHERE peer_device_id = ?1
                 ORDER BY created_at ASC, id ASC",
            )
            .map_err(|err| format!("prepare list_messages failed: {err}"))?;

        let rows = statement
            .query_map(params![peer_device_id], chat_message_from_row)
            .map_err(|err| format!("query list_messages failed: {err}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("read message row failed: {err}"))
    }

    pub fn get_message(&self, message_id: &str) -> Result<Option<ChatMessage>, String> {
        let conn = self.connection()?;
        conn.query_row(
            "SELECT id, peer_device_id, peer_device_name, direction, content, status,
                    content_size, chunk_size, total_chunks, chunks_done,
                    created_at, updated_at, error
             FROM messages
             WHERE id = ?1",
            params![message_id],
            chat_message_from_row,
        )
        .optional()
        .map_err(|err| format!("get message failed: {err}"))
    }

    pub fn save_message(&self, message: &ChatMessage) -> Result<(), String> {
        let conn = self.connection()?;
        conn.execute(
            "INSERT INTO messages (
                id, peer_device_id, peer_device_name, direction, content, status,
                content_size, chunk_size, total_chunks, chunks_done,
                created_at, updated_at, error
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                peer_device_id = excluded.peer_device_id,
                peer_device_name = excluded.peer_device_name,
                direction = excluded.direction,
                content = excluded.content,
                status = excluded.status,
                content_size = excluded.content_size,
                chunk_size = excluded.chunk_size,
                total_chunks = excluded.total_chunks,
                chunks_done = excluded.chunks_done,
                updated_at = excluded.updated_at,
                error = excluded.error",
            params![
                &message.id,
                &message.peer_device_id,
                &message.peer_device_name,
                &message.direction,
                &message.content,
                &message.status,
                message.content_size,
                message.chunk_size,
                message.total_chunks,
                message.chunks_done,
                message.created_at,
                message.updated_at,
                &message.error,
            ],
        )
        .map_err(|err| format!("save message failed: {err}"))?;
        Ok(())
    }

    pub fn prepare_inbound_message(&self, message: &ChatMessage) -> Result<(), String> {
        let conn = self.connection()?;
        conn.execute(
            "DELETE FROM message_chunks WHERE message_id = ?1",
            params![&message.id],
        )
        .map_err(|err| format!("clear old message chunks failed: {err}"))?;
        self.save_message(message)
    }

    pub fn save_message_chunk(
        &self,
        message_id: &str,
        chunk_index: i64,
        bytes: &[u8],
        updated_at: i64,
    ) -> Result<ChatMessage, String> {
        let conn = self.connection()?;
        let message = self
            .get_message(message_id)?
            .ok_or_else(|| "消息不存在，请先 init".to_string())?;

        if message.direction != "inbound" || message.status != "receiving" {
            return Err("消息当前状态不允许接收分片".to_string());
        }

        if chunk_index < 0 || chunk_index >= message.total_chunks {
            return Err("消息分片序号越界".to_string());
        }

        conn.execute(
            "INSERT INTO message_chunks (message_id, chunk_index, content, size, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(message_id, chunk_index) DO UPDATE SET
                content = excluded.content,
                size = excluded.size,
                updated_at = excluded.updated_at",
            params![
                message_id,
                chunk_index,
                bytes,
                i64::try_from(bytes.len()).unwrap_or(i64::MAX),
                updated_at,
            ],
        )
        .map_err(|err| format!("save message chunk failed: {err}"))?;

        let chunks_done: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM message_chunks WHERE message_id = ?1",
                params![message_id],
                |row| row.get(0),
            )
            .map_err(|err| format!("count message chunks failed: {err}"))?;

        conn.execute(
            "UPDATE messages
             SET chunks_done = ?2, updated_at = ?3, error = NULL
             WHERE id = ?1",
            params![message_id, chunks_done, updated_at],
        )
        .map_err(|err| format!("update message chunk progress failed: {err}"))?;

        self.get_message(message_id)?
            .ok_or_else(|| "消息分片已保存但无法重新读取消息".to_string())
    }

    pub fn complete_inbound_message(
        &self,
        message_id: &str,
        updated_at: i64,
    ) -> Result<ChatMessage, String> {
        let conn = self.connection()?;
        let message = self
            .get_message(message_id)?
            .ok_or_else(|| "消息不存在".to_string())?;

        if message.direction != "inbound" || message.status != "receiving" {
            return Err("消息当前状态不允许完成".to_string());
        }

        let chunks_done: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM message_chunks WHERE message_id = ?1",
                params![message_id],
                |row| row.get(0),
            )
            .map_err(|err| format!("count message chunks failed: {err}"))?;

        if chunks_done != message.total_chunks {
            return Err("消息分片不完整".to_string());
        }

        let mut statement = conn
            .prepare(
                "SELECT content
                 FROM message_chunks
                 WHERE message_id = ?1
                 ORDER BY chunk_index ASC",
            )
            .map_err(|err| format!("prepare read message chunks failed: {err}"))?;
        let chunks = statement
            .query_map(params![message_id], |row| row.get::<_, Vec<u8>>(0))
            .map_err(|err| format!("query message chunks failed: {err}"))?;

        let mut content_bytes = Vec::new();
        for chunk in chunks {
            let chunk = chunk.map_err(|err| format!("read message chunk failed: {err}"))?;
            content_bytes.extend(chunk);
        }

        if i64::try_from(content_bytes.len()).unwrap_or(i64::MAX) != message.content_size {
            return Err("消息合并后大小不匹配".to_string());
        }

        let content =
            String::from_utf8(content_bytes).map_err(|_| "消息内容不是有效 UTF-8".to_string())?;

        conn.execute(
            "UPDATE messages
             SET content = ?2, status = 'received', chunks_done = total_chunks,
                 updated_at = ?3, error = NULL
             WHERE id = ?1",
            params![message_id, content, updated_at],
        )
        .map_err(|err| format!("complete inbound message failed: {err}"))?;
        conn.execute(
            "DELETE FROM message_chunks WHERE message_id = ?1",
            params![message_id],
        )
        .map_err(|err| format!("clear completed message chunks failed: {err}"))?;

        self.get_message(message_id)?
            .ok_or_else(|| "消息已完成但无法重新读取消息".to_string())
    }

    pub fn update_message_status(
        &self,
        message_id: &str,
        status: &str,
        chunks_done: i64,
        error: Option<String>,
        updated_at: i64,
    ) -> Result<ChatMessage, String> {
        let conn = self.connection()?;
        conn.execute(
            "UPDATE messages
             SET status = ?2, chunks_done = ?3, error = ?4, updated_at = ?5
             WHERE id = ?1",
            params![message_id, status, chunks_done, error, updated_at],
        )
        .map_err(|err| format!("update message status failed: {err}"))?;

        self.get_message(message_id)?
            .ok_or_else(|| "消息状态已更新但无法重新读取消息".to_string())
    }

    pub fn list_transfers(
        &self,
        peer_device_id: Option<&str>,
    ) -> Result<Vec<FileTransfer>, String> {
        let conn = self.connection()?;
        let sql = match peer_device_id {
            Some(_) => {
                "SELECT id, peer_device_id, peer_device_name, direction, filename, size,
                        status, local_path, created_at, updated_at, error
                 FROM transfers
                 WHERE peer_device_id = ?1
                 ORDER BY created_at ASC, id ASC"
            }
            None => {
                "SELECT id, peer_device_id, peer_device_name, direction, filename, size,
                        status, local_path, created_at, updated_at, error
                 FROM transfers
                 ORDER BY created_at ASC, id ASC"
            }
        };
        let mut statement = conn
            .prepare(sql)
            .map_err(|err| format!("prepare list_transfers failed: {err}"))?;

        let rows = match peer_device_id {
            Some(peer_device_id) => statement
                .query_map(params![peer_device_id], file_transfer_from_row)
                .map_err(|err| format!("query list_transfers failed: {err}"))?,
            None => statement
                .query_map([], file_transfer_from_row)
                .map_err(|err| format!("query list_transfers failed: {err}"))?,
        };

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("read transfer row failed: {err}"))
    }

    pub fn get_transfer(&self, transfer_id: &str) -> Result<Option<FileTransfer>, String> {
        let conn = self.connection()?;
        conn.query_row(
            "SELECT id, peer_device_id, peer_device_name, direction, filename, size,
                    status, local_path, created_at, updated_at, error
             FROM transfers
             WHERE id = ?1",
            params![transfer_id],
            file_transfer_from_row,
        )
        .optional()
        .map_err(|err| format!("get transfer failed: {err}"))
    }

    pub fn save_transfer(&self, transfer: &FileTransfer) -> Result<(), String> {
        let conn = self.connection()?;
        conn.execute(
            "INSERT INTO transfers (
                id, peer_device_id, peer_device_name, direction, filename, size,
                status, local_path, created_at, updated_at, error
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                peer_device_id = excluded.peer_device_id,
                peer_device_name = excluded.peer_device_name,
                direction = excluded.direction,
                filename = excluded.filename,
                size = excluded.size,
                status = excluded.status,
                local_path = excluded.local_path,
                updated_at = excluded.updated_at,
                error = excluded.error",
            params![
                &transfer.id,
                &transfer.peer_device_id,
                &transfer.peer_device_name,
                &transfer.direction,
                &transfer.filename,
                transfer.size,
                &transfer.status,
                &transfer.local_path,
                transfer.created_at,
                transfer.updated_at,
                &transfer.error,
            ],
        )
        .map_err(|err| format!("save transfer failed: {err}"))?;
        Ok(())
    }

    pub fn update_transfer_status(
        &self,
        transfer_id: &str,
        status: &str,
        error: Option<String>,
        updated_at: i64,
    ) -> Result<FileTransfer, String> {
        let conn = self.connection()?;
        conn.execute(
            "UPDATE transfers
             SET status = ?2, error = ?3, updated_at = ?4
             WHERE id = ?1",
            params![transfer_id, status, error, updated_at],
        )
        .map_err(|err| format!("update transfer status failed: {err}"))?;

        self.get_transfer(transfer_id)?
            .ok_or_else(|| "传输状态已更新但无法重新读取记录".to_string())
    }

    fn migrate(&self) -> Result<(), String> {
        let conn = self.connection()?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;

             CREATE TABLE IF NOT EXISTS devices (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                ip TEXT,
                port INTEGER DEFAULT 8765,
                public_key TEXT,
                trusted INTEGER DEFAULT 1,
                platform TEXT,
                version TEXT,
                protocol_version INTEGER,
                features TEXT,
                online INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                last_seen_at INTEGER,
                last_checked_at INTEGER,
                last_error TEXT
             );

             CREATE UNIQUE INDEX IF NOT EXISTS idx_devices_ip_port ON devices(ip, port);
             CREATE INDEX IF NOT EXISTS idx_devices_trusted ON devices(trusted);
             CREATE INDEX IF NOT EXISTS idx_devices_online ON devices(online);

             CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                peer_device_id TEXT NOT NULL,
                peer_device_name TEXT,
                direction TEXT NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL,
                content_size INTEGER NOT NULL,
                chunk_size INTEGER NOT NULL,
                total_chunks INTEGER NOT NULL,
                chunks_done INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                error TEXT
             );

             CREATE INDEX IF NOT EXISTS idx_messages_peer_created
                ON messages(peer_device_id, created_at);
             CREATE INDEX IF NOT EXISTS idx_messages_status ON messages(status);

             CREATE TABLE IF NOT EXISTS message_chunks (
                message_id TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                content BLOB NOT NULL,
                size INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (message_id, chunk_index),
                FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
             );

             CREATE INDEX IF NOT EXISTS idx_message_chunks_message
                ON message_chunks(message_id);

             CREATE TABLE IF NOT EXISTS transfers (
                id TEXT PRIMARY KEY,
                peer_device_id TEXT NOT NULL,
                peer_device_name TEXT,
                direction TEXT NOT NULL,
                filename TEXT NOT NULL,
                size INTEGER NOT NULL,
                status TEXT NOT NULL,
                local_path TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                error TEXT
             );

             CREATE INDEX IF NOT EXISTS idx_transfers_peer_created
                ON transfers(peer_device_id, created_at);
             CREATE INDEX IF NOT EXISTS idx_transfers_status ON transfers(status);",
        )
        .map_err(|err| format!("run sqlite migrations failed: {err}"))?;

        Ok(())
    }

    fn connection(&self) -> Result<Connection, String> {
        Connection::open(&self.db_path).map_err(|err| format!("open sqlite database failed: {err}"))
    }
}

fn chat_message_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChatMessage> {
    Ok(ChatMessage {
        id: row.get(0)?,
        peer_device_id: row.get(1)?,
        peer_device_name: row.get(2)?,
        direction: row.get(3)?,
        content: row.get(4)?,
        status: row.get(5)?,
        content_size: row.get(6)?,
        chunk_size: row.get(7)?,
        total_chunks: row.get(8)?,
        chunks_done: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        error: row.get(12)?,
    })
}

fn file_transfer_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileTransfer> {
    Ok(FileTransfer {
        id: row.get(0)?,
        peer_device_id: row.get(1)?,
        peer_device_name: row.get(2)?,
        direction: row.get(3)?,
        filename: row.get(4)?,
        size: row.get(5)?,
        status: row.get(6)?,
        local_path: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        error: row.get(10)?,
    })
}
