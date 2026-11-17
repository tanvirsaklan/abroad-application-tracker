//! SQLite persistence.
//!
//! Every statement uses bound parameters; no SQL is ever built from user text.
//! The table layout is identical to the legacy Python app, so an existing
//! `professor_applications.db` can be opened as-is.

use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use directories::ProjectDirs;
use rusqlite::{params, Connection, Row};

use crate::error::{AppError, Result};
use crate::model::{Application, ApplicationInput, Response, Status, DATE_FORMAT};

pub const DB_ENV_VAR: &str = "PROFESSOR_TRACKER_DB";
const DB_FILE_NAME: &str = "abroad_applications.db";

/// Where the database lives: `$PROFESSOR_TRACKER_DB` if set, otherwise the
/// per-user data directory (no root permissions or `chmod 777` needed).
pub fn default_db_path() -> PathBuf {
    if let Some(custom) = std::env::var_os(DB_ENV_VAR) {
        return PathBuf::from(custom);
    }
    ProjectDirs::from("", "", "professor-tracker")
        .map(|dirs| dirs.data_dir().join(DB_FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(DB_FILE_NAME))
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
            restrict_permissions(parent, 0o700);
        }
        let conn = Connection::open(path)?;
        restrict_permissions(path, 0o600);
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(3))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS applications (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                professor_name TEXT NOT NULL,
                professor_email TEXT NOT NULL,
                university_name TEXT NOT NULL,
                program_name TEXT NOT NULL,
                country_name TEXT NOT NULL,
                application_last_date TEXT NOT NULL,
                email_subject TEXT,
                email_body TEXT,
                email_send_date TEXT,
                status TEXT DEFAULT 'pending',
                response TEXT DEFAULT 'no'
            );
            CREATE INDEX IF NOT EXISTS idx_applications_deadline
                ON applications(application_last_date);",
        )?;
        Ok(Self { conn })
    }

    pub fn all(&self) -> Result<Vec<Application>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, professor_name, professor_email, university_name, program_name,
                    country_name, application_last_date, email_subject, email_body,
                    email_send_date, status, response
             FROM applications ORDER BY id",
        )?;
        let rows = stmt.query_map([], row_to_application)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn insert(&self, a: &ApplicationInput) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO applications
                (professor_name, professor_email, university_name, program_name, country_name,
                 application_last_date, email_subject, email_body, email_send_date, status, response)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                a.professor_name,
                a.professor_email,
                a.university_name,
                a.program_name,
                a.country_name,
                fmt_date(a.application_last_date),
                a.email_subject,
                a.email_body,
                fmt_date(a.email_send_date),
                a.status.as_db_str(),
                a.response.as_db_str(),
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update(&self, id: i64, a: &ApplicationInput) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE applications SET
                professor_name = ?1, professor_email = ?2, university_name = ?3,
                program_name = ?4, country_name = ?5, application_last_date = ?6,
                email_subject = ?7, email_body = ?8, email_send_date = ?9,
                status = ?10, response = ?11
             WHERE id = ?12",
            params![
                a.professor_name,
                a.professor_email,
                a.university_name,
                a.program_name,
                a.country_name,
                fmt_date(a.application_last_date),
                a.email_subject,
                a.email_body,
                fmt_date(a.email_send_date),
                a.status.as_db_str(),
                a.response.as_db_str(),
                id,
            ],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(id));
        }
        Ok(())
    }

    /// Change only the status, leaving every other column untouched
    /// (safe even for legacy rows that would fail today's validation).
    pub fn set_status(&self, id: i64, status: Status) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE applications SET status = ?1 WHERE id = ?2",
            params![status.as_db_str(), id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(id));
        }
        Ok(())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let changed = self
            .conn
            .execute("DELETE FROM applications WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(AppError::NotFound(id));
        }
        Ok(())
    }
}

fn fmt_date(d: NaiveDate) -> String {
    d.format(DATE_FORMAT).to_string()
}

fn parse_date(s: Option<String>) -> Option<NaiveDate> {
    s.and_then(|s| NaiveDate::parse_from_str(s.trim(), DATE_FORMAT).ok())
}

fn row_to_application(row: &Row<'_>) -> rusqlite::Result<Application> {
    // Columns that were nullable in the legacy schema are read as Option.
    let text = |idx: usize| -> rusqlite::Result<String> {
        Ok(row.get::<_, Option<String>>(idx)?.unwrap_or_default())
    };
    Ok(Application {
        id: row.get(0)?,
        professor_name: text(1)?,
        professor_email: text(2)?,
        university_name: text(3)?,
        program_name: text(4)?,
        country_name: text(5)?,
        application_last_date: parse_date(row.get(6)?),
        email_subject: text(7)?,
        email_body: text(8)?,
        email_send_date: parse_date(row.get(9)?),
        status: Status::from_db_str(&text(10)?),
        response: Response::from_db_str(&text(11)?),
    })
}

/// Best-effort: keep the data private to the current user on Unix.
#[cfg(unix)]
fn restrict_permissions(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path, _mode: u32) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str) -> ApplicationInput {
        ApplicationInput {
            professor_name: name.into(),
            professor_email: "p@uni.edu".into(),
            university_name: "Uni".into(),
            program_name: "MSc".into(),
            country_name: "Canada".into(),
            ..ApplicationInput::blank(NaiveDate::from_ymd_opt(2026, 12, 1).unwrap())
        }
    }

    #[test]
    fn insert_list_update_delete_roundtrip() {
        let db = Database::open_in_memory().unwrap();
        let id = db.insert(&sample("Ada")).unwrap();
        let rows = db.all().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].application_last_date, NaiveDate::from_ymd_opt(2026, 12, 1));

        let mut edited = sample("Ada L.");
        edited.status = Status::Done;
        edited.response = Response::Yes;
        db.update(id, &edited).unwrap();
        let rows = db.all().unwrap();
        assert_eq!(rows[0].professor_name, "Ada L.");
        assert_eq!(rows[0].status, Status::Done);
        assert_eq!(rows[0].response, Response::Yes);

        db.delete(id).unwrap();
        assert!(db.all().unwrap().is_empty());
        assert!(matches!(db.delete(id), Err(AppError::NotFound(_))));
        assert!(matches!(db.update(id, &edited), Err(AppError::NotFound(_))));
    }

    #[test]
    fn set_status_changes_only_status() {
        let db = Database::open_in_memory().unwrap();
        let id = db.insert(&sample("Ada")).unwrap();
        db.set_status(id, Status::Done).unwrap();
        let rows = db.all().unwrap();
        assert_eq!(rows[0].status, Status::Done);
        assert_eq!(rows[0].professor_name, "Ada");
        assert!(matches!(db.set_status(999, Status::Done), Err(AppError::NotFound(999))));
    }

    #[test]
    fn hostile_text_is_stored_as_plain_data() {
        let db = Database::open_in_memory().unwrap();
        let nasty = "Robert'); DROP TABLE applications;--";
        db.insert(&sample(nasty)).unwrap();
        let rows = db.all().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].professor_name, nasty);
    }

    #[test]
    fn legacy_rows_with_nulls_and_bad_dates_still_load() {
        let db = Database::open_in_memory().unwrap();
        db.conn
            .execute(
                "INSERT INTO applications
                 (professor_name, professor_email, university_name, program_name,
                  country_name, application_last_date, status, response)
                 VALUES ('A','a@b.co','U','P','C','not-a-date', 'weird', NULL)",
                [],
            )
            .unwrap();
        let rows = db.all().unwrap();
        assert_eq!(rows[0].application_last_date, None);
        assert_eq!(rows[0].email_subject, "");
        assert_eq!(rows[0].status, Status::Pending);
        assert_eq!(rows[0].response, Response::No);
    }
}
