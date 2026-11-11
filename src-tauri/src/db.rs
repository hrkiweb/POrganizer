//! Local SQLite storage.
//!
//! The UI's data is stored as *records* (one row per day page, contact, task,
//! project, note, link, ...) with a JSON payload. Every record carries the
//! columns a future sync server needs: the space it belongs to, a hybrid
//! logical clock, who changed it, and a tombstone flag. Every write is also
//! appended to `changes`, which becomes the outbox once server sync exists.
//!
//! Calendar tables (accounts, calendars, events, attendees, reminders) are
//! defined here for the Google Calendar / iCloud (CalDAV) integration. See
//! docs/DATA_MODEL.md for the field mapping.

use rusqlite::{backup::Backup, params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct Db {
    pub conn: Mutex<Connection>,
    pub path: PathBuf,
}

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS meta_kv (
  key   TEXT PRIMARY KEY,
  value TEXT
);

-- A space is the unit of sharing. 'personal' always exists; shared spaces
-- arrive with server mode.
CREATE TABLE IF NOT EXISTS spaces (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  kind       TEXT NOT NULL DEFAULT 'personal' CHECK (kind IN ('personal','shared')),
  owner_id   TEXT,
  server_id  TEXT,
  role       TEXT CHECK (role IN ('owner','editor','viewer')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
INSERT OR IGNORE INTO spaces (id, name, kind, role, created_at, updated_at)
VALUES ('personal', 'Personal', 'personal', 'owner', strftime('%Y-%m-%dT%H:%M:%fZ','now'), strftime('%Y-%m-%dT%H:%M:%fZ','now'));

-- All organizer data. type: day | week | month | contact | note | project |
-- task | call | anniv | planner | link | meta
CREATE TABLE IF NOT EXISTS records (
  id         TEXT PRIMARY KEY,
  space_id   TEXT NOT NULL DEFAULT 'personal' REFERENCES spaces(id),
  type       TEXT NOT NULL,
  data       TEXT,                 -- JSON payload (NULL when deleted)
  date_from  TEXT,                 -- for calendar range queries (YYYY-MM-DD)
  date_to    TEXT,
  parent_id  TEXT,                 -- e.g. project of a task
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  updated_by TEXT,                 -- device id now, user id in server mode
  hlc        TEXT NOT NULL,        -- hybrid logical clock: <ms:13>-<counter:6>-<device>
  deleted    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_records_type   ON records (type, deleted);
CREATE INDEX IF NOT EXISTS idx_records_dates  ON records (date_from, date_to);
CREATE INDEX IF NOT EXISTS idx_records_parent ON records (parent_id);
CREATE INDEX IF NOT EXISTS idx_records_space  ON records (space_id, hlc);

-- Outbox / history of local writes (pushed = sent to the server).
CREATE TABLE IF NOT EXISTS changes (
  seq       INTEGER PRIMARY KEY AUTOINCREMENT,
  record_id TEXT NOT NULL,
  op        TEXT NOT NULL CHECK (op IN ('upsert','delete')),
  hlc       TEXT NOT NULL,
  device    TEXT,
  pushed    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_changes_pushed ON changes (pushed, seq);

-- Attachment files (content-addressed once server sync uploads them).
CREATE TABLE IF NOT EXISTS blobs (
  sha256   TEXT PRIMARY KEY,
  file     TEXT NOT NULL,
  size     INTEGER,
  mime     TEXT,
  uploaded INTEGER NOT NULL DEFAULT 0
);

-- ===== External calendars (Google Calendar, iCloud / CalDAV) =====
CREATE TABLE IF NOT EXISTS calendar_accounts (
  id             TEXT PRIMARY KEY,
  provider       TEXT NOT NULL CHECK (provider IN ('local','google','icloud','caldav')),
  display_name   TEXT,
  username       TEXT,             -- Google e-mail or Apple ID
  server_url     TEXT,             -- CalDAV base URL (iCloud: https://caldav.icloud.com)
  credential_ref TEXT,             -- key in the OS keychain; secrets are never stored here
  sync_mode      TEXT NOT NULL DEFAULT 'read' CHECK (sync_mode IN ('read','two_way')),
  status         TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','paused','error','auth_required')),
  last_sync_at   TEXT,
  last_error     TEXT,
  created_at     TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS calendars (
  id           TEXT PRIMARY KEY,
  account_id   TEXT REFERENCES calendar_accounts(id) ON DELETE CASCADE,  -- NULL = Organizer's own calendar
  space_id     TEXT REFERENCES spaces(id),
  remote_id    TEXT,               -- Google calendarId / CalDAV collection href
  name         TEXT NOT NULL,
  description  TEXT,
  color        TEXT,
  timezone     TEXT,               -- IANA, e.g. Europe/Zagreb
  access_role  TEXT CHECK (access_role IN ('owner','writer','reader','freebusy')),
  visible      INTEGER NOT NULL DEFAULT 1,
  is_primary   INTEGER NOT NULL DEFAULT 0,
  sync_token   TEXT,               -- Google nextSyncToken / WebDAV sync-token (RFC 6578)
  ctag         TEXT,               -- CalDAV getctag (CalendarServer extension)
  last_sync_at TEXT,
  UNIQUE (account_id, remote_id)
);
INSERT OR IGNORE INTO calendars (id, account_id, space_id, name, color, access_role, is_primary)
VALUES ('local', NULL, 'personal', 'Organizer', '#9c7a63', 'owner', 1);

CREATE TABLE IF NOT EXISTS events (
  id                 TEXT PRIMARY KEY,
  calendar_id        TEXT NOT NULL REFERENCES calendars(id) ON DELETE CASCADE,
  remote_id          TEXT,          -- Google event id / CalDAV resource href
  ical_uid           TEXT,          -- iCalendar UID (same on both providers)
  etag               TEXT,
  remote_updated     TEXT,
  title              TEXT,
  description        TEXT,
  location           TEXT,
  status             TEXT NOT NULL DEFAULT 'confirmed' CHECK (status IN ('confirmed','tentative','cancelled')),
  transparency       TEXT NOT NULL DEFAULT 'opaque' CHECK (transparency IN ('opaque','transparent')),
  visibility         TEXT NOT NULL DEFAULT 'default' CHECK (visibility IN ('default','public','private','confidential')),
  all_day            INTEGER NOT NULL DEFAULT 0,
  start_at           TEXT NOT NULL, -- UTC ISO-8601 for timed events; local midnight for all-day
  end_at             TEXT NOT NULL, -- exclusive
  start_date         TEXT,          -- YYYY-MM-DD for all-day events
  end_date           TEXT,          -- exclusive, YYYY-MM-DD
  timezone           TEXT,          -- IANA zone of DTSTART
  rrule              TEXT,          -- e.g. FREQ=WEEKLY;BYDAY=MO
  exdates            TEXT,          -- JSON array of excluded starts
  rdates             TEXT,          -- JSON array of extra starts
  recurring_event_id TEXT,          -- master event for an overridden instance
  original_start     TEXT,          -- RECURRENCE-ID / originalStartTime
  organizer_email    TEXT,
  organizer_name     TEXT,
  color              TEXT,
  url                TEXT,          -- Google htmlLink
  raw                TEXT,          -- original JSON (Google) or ICS (CalDAV), kept for lossless round-trips
  local_record_id    TEXT,          -- link to an Organizer record (e.g. day slot) if created here
  dirty              INTEGER NOT NULL DEFAULT 0,  -- changed locally, needs push
  deleted            INTEGER NOT NULL DEFAULT 0,
  created_at         TEXT NOT NULL,
  updated_at         TEXT NOT NULL,
  UNIQUE (calendar_id, remote_id)
);
CREATE INDEX IF NOT EXISTS idx_events_range ON events (calendar_id, start_at, end_at);
CREATE INDEX IF NOT EXISTS idx_events_uid   ON events (ical_uid);
CREATE INDEX IF NOT EXISTS idx_events_dirty ON events (dirty);

CREATE TABLE IF NOT EXISTS event_attendees (
  event_id     TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
  email        TEXT NOT NULL,
  name         TEXT,
  role         TEXT CHECK (role IN ('required','optional','chair','non-participant')),
  status       TEXT CHECK (status IN ('needs-action','accepted','declined','tentative','delegated')),
  is_organizer INTEGER NOT NULL DEFAULT 0,
  is_self      INTEGER NOT NULL DEFAULT 0,
  contact_id   TEXT,               -- matching Organizer contact, if any
  PRIMARY KEY (event_id, email)
);

CREATE TABLE IF NOT EXISTS event_reminders (
  event_id       TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
  method         TEXT NOT NULL CHECK (method IN ('popup','email','display','audio')),
  minutes_before INTEGER NOT NULL,
  PRIMARY KEY (event_id, method, minutes_before)
);

CREATE TABLE IF NOT EXISTS calendar_sync_log (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id  TEXT,
  calendar_id TEXT,
  started_at  TEXT NOT NULL,
  finished_at TEXT,
  direction   TEXT CHECK (direction IN ('pull','push','both')),
  pulled      INTEGER DEFAULT 0,
  pushed      INTEGER DEFAULT 0,
  error       TEXT
);
"#;

/// v2: Organizer's own event fields (categories, local times, …) and sync clock on events.
const SCHEMA_V2: &str = r#"
ALTER TABLE events ADD COLUMN app_data   TEXT;  -- Organizer JSON of the event (categories, local start/end, …)
ALTER TABLE events ADD COLUMN hlc        TEXT;
ALTER TABLE events ADD COLUMN updated_by TEXT;
CREATE INDEX IF NOT EXISTS idx_events_days ON events (calendar_id, start_date, end_date);
PRAGMA user_version = 2;
"#;

fn err<E: ToString>(e: E) -> String {
    e.to_string()
}

pub fn open(path: &Path) -> Result<Connection, String> {
    let c = Connection::open(path).map_err(err)?;
    c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=NORMAL;")
        .map_err(err)?;
    migrate(&c)?;
    Ok(c)
}

fn migrate(c: &Connection) -> Result<(), String> {
    let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(err)?;
    if v < 1 {
        c.execute_batch(SCHEMA_V1).map_err(err)?;
        c.execute_batch("PRAGMA user_version = 1").map_err(err)?;
    }
    if v < 2 {
        c.execute_batch(SCHEMA_V2).map_err(err)?;
    }
    Ok(())
}

static HLC_COUNTER: AtomicU64 = AtomicU64::new(0);
fn hlc(device: &str) -> String {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let n = HLC_COUNTER.fetch_add(1, Ordering::Relaxed) % 1_000_000;
    format!("{:013}-{:06}-{}", ms, n, device)
}

#[derive(Serialize)]
pub struct Rec {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    data: String,
}

#[derive(Deserialize)]
pub struct Upsert {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    data: String,
    date_from: Option<String>,
    date_to: Option<String>,
    parent_id: Option<String>,
}

#[tauri::command]
pub fn db_load(db: tauri::State<'_, Db>) -> Result<Vec<Rec>, String> {
    let c = db.conn.lock().map_err(err)?;
    let mut st = c
        .prepare("SELECT id, type, data FROM records WHERE deleted = 0 AND data IS NOT NULL")
        .map_err(err)?;
    let rows = st
        .query_map([], |r| Ok(Rec { id: r.get(0)?, kind: r.get(1)?, data: r.get(2)? }))
        .map_err(err)?;
    let mut out = rows.collect::<Result<Vec<_>, _>>().map_err(err)?;
    let mut st = c
        .prepare("SELECT id, app_data FROM events WHERE deleted = 0 AND calendar_id = 'local' AND app_data IS NOT NULL")
        .map_err(err)?;
    let evs = st
        .query_map([], |r| {
            Ok(Rec { id: format!("event:{}", r.get::<_, String>(0)?), kind: "event".into(), data: r.get(1)? })
        })
        .map_err(err)?;
    for e in evs {
        out.push(e.map_err(err)?);
    }
    Ok(out)
}

fn s_of(v: &serde_json::Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).filter(|x| !x.is_empty()).map(|x| x.to_string())
}

/// Organizer event JSON -> row of the `events` table (calendar 'local').
fn upsert_event(tx: &rusqlite::Transaction, id: &str, data: &str, now: &str, device: &str, h: &str) -> Result<(), String> {
    let v: serde_json::Value = serde_json::from_str(data).map_err(err)?;
    let all_day = v.get("allDay").and_then(|x| x.as_bool()).unwrap_or(false);
    let start_at = s_of(&v, "_startAt").or_else(|| s_of(&v, "start")).unwrap_or_default();
    let end_at = s_of(&v, "_endAt").or_else(|| s_of(&v, "end")).unwrap_or_else(|| start_at.clone());
    let exd = v.get("exdates").filter(|x| x.as_array().map_or(false, |a| !a.is_empty())).map(|x| x.to_string());
    let created = s_of(&v, "created").unwrap_or_else(|| now.to_string());
    tx.execute(
        "INSERT INTO events (id, calendar_id, ical_uid, title, description, location, all_day, start_at, end_at,
            start_date, end_date, timezone, rrule, exdates, color, app_data, dirty, deleted, created_at, updated_at, hlc, updated_by)
         VALUES (?1, 'local', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, 1, 0, ?16, ?17, ?18, ?19)
         ON CONFLICT(id) DO UPDATE SET title = excluded.title, description = excluded.description, location = excluded.location,
            all_day = excluded.all_day, start_at = excluded.start_at, end_at = excluded.end_at, start_date = excluded.start_date,
            end_date = excluded.end_date, timezone = excluded.timezone, rrule = excluded.rrule, exdates = excluded.exdates,
            color = excluded.color, app_data = excluded.app_data, dirty = 1, deleted = 0, updated_at = excluded.updated_at,
            hlc = excluded.hlc, updated_by = excluded.updated_by",
        params![
            id,
            format!("{}@organizer", id),
            s_of(&v, "title"),
            s_of(&v, "desc"),
            s_of(&v, "loc"),
            all_day as i64,
            start_at,
            end_at,
            s_of(&v, "_startDate"),
            s_of(&v, "_endDate"),
            s_of(&v, "_tz"),
            s_of(&v, "rrule"),
            exd,
            s_of(&v, "color"),
            data,
            created,
            now,
            h,
            device
        ],
    )
    .map_err(err)?;
    Ok(())
}

#[tauri::command]
pub fn db_apply(
    db: tauri::State<'_, Db>,
    upserts: Vec<Upsert>,
    deletes: Vec<String>,
    now: String,
    device: String,
) -> Result<usize, String> {
    let mut c = db.conn.lock().map_err(err)?;
    let tx = c.transaction().map_err(err)?;
    {
        let mut up = tx
            .prepare(
                "INSERT INTO records (id, space_id, type, data, date_from, date_to, parent_id, created_at, updated_at, updated_by, hlc, deleted)
                 VALUES (?1, 'personal', ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, ?9, 0)
                 ON CONFLICT(id) DO UPDATE SET type = excluded.type, data = excluded.data, date_from = excluded.date_from,
                   date_to = excluded.date_to, parent_id = excluded.parent_id, updated_at = excluded.updated_at,
                   updated_by = excluded.updated_by, hlc = excluded.hlc, deleted = 0",
            )
            .map_err(err)?;
        let mut del = tx
            .prepare("UPDATE records SET deleted = 1, data = NULL, updated_at = ?2, updated_by = ?3, hlc = ?4 WHERE id = ?1")
            .map_err(err)?;
        let mut log = tx
            .prepare("INSERT INTO changes (record_id, op, hlc, device) VALUES (?1, ?2, ?3, ?4)")
            .map_err(err)?;
        for u in &upserts {
            let h = hlc(&device);
            if let Some(eid) = u.id.strip_prefix("event:") {
                upsert_event(&tx, eid, &u.data, &now, &device, &h)?;
            } else {
                up.execute(params![u.id, u.kind, u.data, u.date_from, u.date_to, u.parent_id, now, device, h])
                    .map_err(err)?;
            }
            log.execute(params![u.id, "upsert", h, device]).map_err(err)?;
        }
        for id in &deletes {
            let h = hlc(&device);
            if let Some(eid) = id.strip_prefix("event:") {
                tx.execute(
                    "UPDATE events SET deleted = 1, dirty = 1, app_data = NULL, updated_at = ?2, updated_by = ?3, hlc = ?4 WHERE id = ?1",
                    params![eid, now, device, h],
                )
                .map_err(err)?;
            } else {
                del.execute(params![id, now, device, h]).map_err(err)?;
            }
            log.execute(params![id, "delete", h, device]).map_err(err)?;
        }
    }
    tx.commit().map_err(err)?;
    Ok(upserts.len() + deletes.len())
}

#[tauri::command]
pub fn kv_get(db: tauri::State<'_, Db>, key: String) -> Result<Option<String>, String> {
    let c = db.conn.lock().map_err(err)?;
    c.query_row("SELECT value FROM meta_kv WHERE key = ?1", params![key], |r| r.get(0))
        .optional()
        .map_err(err)
}

#[tauri::command]
pub fn kv_set(db: tauri::State<'_, Db>, key: String, value: String) -> Result<(), String> {
    let c = db.conn.lock().map_err(err)?;
    c.execute(
        "INSERT INTO meta_kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(err)?;
    Ok(())
}

#[derive(Serialize)]
pub struct DbInfo {
    path: String,
    size: u64,
    records: i64,
    changes: i64,
    schema_version: i64,
}

#[tauri::command]
pub fn db_info(db: tauri::State<'_, Db>) -> Result<DbInfo, String> {
    let c = db.conn.lock().map_err(err)?;
    let records: i64 = c
        .query_row(
            "SELECT (SELECT COUNT(*) FROM records WHERE deleted = 0) + (SELECT COUNT(*) FROM events WHERE deleted = 0)",
            [],
            |r| r.get(0),
        )
        .map_err(err)?;
    let changes: i64 = c.query_row("SELECT COUNT(*) FROM changes", [], |r| r.get(0)).map_err(err)?;
    let schema_version: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(err)?;
    let mut size = std::fs::metadata(&db.path).map(|m| m.len()).unwrap_or(0);
    if let Ok(m) = std::fs::metadata(db.path.with_extension("db-wal")) {
        size += m.len();
    }
    Ok(DbInfo { path: db.path.to_string_lossy().to_string(), size, records, changes, schema_version })
}

#[derive(Serialize)]
pub struct BackupInfo {
    file: String,
    size: u64,
    files_copied: u32,
    removed: u32,
}

/// Snapshot the database into `folder` (e.g. a Google Drive / OneDrive /
/// iCloud Drive folder), mirror new attachment files, keep the newest `keep`.
pub fn backup_to(db: &Db, attachments: &Path, folder: &str, stamp: &str, keep: usize) -> Result<BackupInfo, String> {
    let dir = PathBuf::from(folder);
    std::fs::create_dir_all(&dir).map_err(err)?;
    let safe: String = stamp.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    let dest = dir.join(format!("Organizer_{}.db", safe));
    let tmp = dir.join(format!(".Organizer_{}.db.part", safe));
    {
        let c = db.conn.lock().map_err(err)?;
        let mut out = Connection::open(&tmp).map_err(err)?;
        {
            let b = Backup::new(&c, &mut out).map_err(err)?;
            b.run_to_completion(512, Duration::from_millis(0), None).map_err(err)?;
        }
        // single self-contained file: no -wal / -shm companions in the cloud folder
        out.query_row("PRAGMA journal_mode=DELETE", [], |_| Ok(())).map_err(err)?;
    }
    std::fs::rename(&tmp, &dest).map_err(err)?;
    let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);

    // attachments: copy files that are not in the backup yet (never deletes)
    let mut files_copied = 0;
    if attachments.is_dir() {
        let target = dir.join("attachments");
        std::fs::create_dir_all(&target).map_err(err)?;
        for e in std::fs::read_dir(attachments).map_err(err)?.flatten() {
            let p = e.path();
            if p.is_file() {
                let t = target.join(e.file_name());
                if !t.exists() {
                    std::fs::copy(&p, &t).map_err(err)?;
                    files_copied += 1;
                }
            }
        }
    }

    // keep only the newest `keep` snapshots
    let mut snaps: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(err)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with("Organizer_") && n.ends_with(".db")).unwrap_or(false)
        })
        .collect();
    snaps.sort();
    let mut removed = 0;
    while snaps.len() > keep.max(1) {
        let old = snaps.remove(0);
        if std::fs::remove_file(&old).is_ok() {
            removed += 1;
        }
        for ext in ["db-wal", "db-shm"] {
            let _ = std::fs::remove_file(old.with_extension(ext));
        }
    }
    Ok(BackupInfo { file: dest.to_string_lossy().to_string(), size, files_copied, removed })
}

/// Replace the current database with a backup file. A safety copy of the
/// current state is written next to the live database first.
pub fn restore_from(db: &Db, file: &str) -> Result<(), String> {
    let src = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(err)?;
    let ok: i64 = src
        .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'records'", [], |r| r.get(0))
        .map_err(err)?;
    if ok != 1 {
        return Err("not an Organizer database".into());
    }
    let mut c = db.conn.lock().map_err(err)?;
    {
        let safety = db.path.with_file_name("organizer-before-restore.db");
        let _ = std::fs::remove_file(&safety);
        let mut out = Connection::open(&safety).map_err(err)?;
        {
            let b = Backup::new(&c, &mut out).map_err(err)?;
            b.run_to_completion(512, Duration::from_millis(0), None).map_err(err)?;
        }
        out.query_row("PRAGMA journal_mode=DELETE", [], |_| Ok(())).map_err(err)?;
    }
    {
        let b = Backup::new(&src, &mut c).map_err(err)?;
        b.run_to_completion(512, Duration::from_millis(0), None).map_err(err)?;
    }
    migrate(&c)?;
    Ok(())
}
