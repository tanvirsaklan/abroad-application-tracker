//! CSV export.
//!
//! Spreadsheet apps execute cells that start with `=`, `+`, `-` or `@` as
//! formulas ("CSV injection"). Any such cell is prefixed with an apostrophe so
//! hostile text in a professor's name or an email body can't run when the file
//! is opened in Excel/LibreOffice.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDate};
use directories::UserDirs;

use crate::error::Result;
use crate::model::{Application, DATE_FORMAT};

const HEADERS: [&str; 11] = [
    "Professor Name",
    "Professor Email",
    "University",
    "Program",
    "Country",
    "Application Last Date",
    "Email Subject",
    "Email Body",
    "Email Send Date",
    "Status",
    "Response",
];

pub fn sanitize_cell(value: &str) -> String {
    match value.chars().next() {
        Some('=' | '+' | '-' | '@' | '\t' | '\r') => format!("'{value}"),
        _ => value.to_owned(),
    }
}

fn date_cell(d: Option<NaiveDate>) -> String {
    d.map(|d| d.format(DATE_FORMAT).to_string()).unwrap_or_default()
}

pub fn write_csv<W: Write>(writer: W, apps: &[&Application]) -> Result<()> {
    let mut csv = csv::Writer::from_writer(writer);
    csv.write_record(HEADERS)?;
    for a in apps {
        csv.write_record([
            sanitize_cell(&a.professor_name),
            sanitize_cell(&a.professor_email),
            sanitize_cell(&a.university_name),
            sanitize_cell(&a.program_name),
            sanitize_cell(&a.country_name),
            date_cell(a.application_last_date),
            sanitize_cell(&a.email_subject),
            sanitize_cell(&a.email_body),
            date_cell(a.email_send_date),
            a.status.label().to_owned(),
            a.response.label().to_owned(),
        ])?;
    }
    csv.flush()?;
    Ok(())
}

/// `~/Documents` if it exists, else the home directory, else the working directory.
pub fn default_export_dir() -> PathBuf {
    UserDirs::new()
        .map(|d| d.document_dir().map(Path::to_path_buf).unwrap_or_else(|| d.home_dir().to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Write to a fresh timestamped file in `dir`. `create_new` guarantees an
/// existing file is never overwritten.
pub fn export_to_dir(dir: &Path, apps: &[&Application]) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stamp = Local::now().format("%Y%m%d_%H%M%S");
    let path = dir.join(format!("abroad_applications_{stamp}.csv"));

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&path)?;
    write_csv(file, apps)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Response, Status};

    fn app(name: &str) -> Application {
        Application {
            id: 1,
            professor_name: name.into(),
            professor_email: "a@b.edu".into(),
            university_name: "U, Inc".into(),
            program_name: "P".into(),
            country_name: "C".into(),
            application_last_date: NaiveDate::from_ymd_opt(2026, 12, 1),
            email_subject: "Hi".into(),
            email_body: "line1\nline2 \"quoted\"".into(),
            email_send_date: None,
            status: Status::Done,
            response: Response::Partial,
        }
    }

    #[test]
    fn neutralises_formula_cells() {
        assert_eq!(sanitize_cell("=1+1"), "'=1+1");
        assert_eq!(sanitize_cell("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(sanitize_cell("-5"), "'-5");
        assert_eq!(sanitize_cell("Ada"), "Ada");
        assert_eq!(sanitize_cell(""), "");
    }

    #[test]
    fn csv_roundtrip_with_commas_quotes_and_newlines() {
        let a = app("=cmd|' /C calc'!A0");
        let mut buf = Vec::new();
        write_csv(&mut buf, &[&a]).unwrap();

        let mut reader = csv::Reader::from_reader(buf.as_slice());
        let row = reader.records().next().unwrap().unwrap();
        assert_eq!(&row[0], "'=cmd|' /C calc'!A0");
        assert_eq!(&row[2], "U, Inc");
        assert_eq!(&row[5], "2026-12-01");
        assert_eq!(&row[7], "line1\nline2 \"quoted\"");
        assert_eq!(&row[8], "");
        assert_eq!((&row[9], &row[10]), ("Done", "Partial"));
    }

    #[test]
    fn export_never_overwrites() {
        let dir = std::env::temp_dir().join(format!("pt_export_test_{}", std::process::id()));
        let a = app("Ada");
        let path = export_to_dir(&dir, &[&a]).unwrap();
        assert!(path.exists());
        // Same-second second export collides on the name and must fail, not clobber.
        let again = export_to_dir(&dir, &[&a]);
        assert!(again.is_err() || again.unwrap() != path);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
