//! Domain types and input validation.
//!
//! Everything the user types goes through [`ApplicationInput::validate`] before
//! it can reach the database, so the rules live in exactly one place.

use chrono::{Datelike, NaiveDate};

/// Date format used in the database (identical to the legacy Python app,
/// so an existing `professor_applications.db` keeps working).
pub const DATE_FORMAT: &str = "%Y-%m-%d";

pub const MAX_SHORT_TEXT: usize = 200;
pub const MAX_EMAIL: usize = 254;
pub const MAX_SUBJECT: usize = 300;
pub const MAX_BODY: usize = 20_000;

const MIN_YEAR: i32 = 2000;
const MAX_YEAR: i32 = 2100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Pending,
    Done,
}

impl Status {
    pub const ALL: [Status; 2] = [Status::Pending, Status::Done];

    pub fn as_db_str(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Done => "done",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::Pending => "Pending",
            Status::Done => "Done",
        }
    }

    /// Lenient parse: unknown or legacy values fall back to `Pending`
    /// instead of crashing the app.
    pub fn from_db_str(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("done") {
            Status::Done
        } else {
            Status::Pending
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Response {
    No,
    Yes,
    Partial,
}

impl Response {
    pub const ALL: [Response; 3] = [Response::No, Response::Yes, Response::Partial];

    pub fn as_db_str(self) -> &'static str {
        match self {
            Response::No => "no",
            Response::Yes => "yes",
            Response::Partial => "partial",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Response::No => "No",
            Response::Yes => "Yes",
            Response::Partial => "Partial",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("yes") {
            Response::Yes
        } else if s.eq_ignore_ascii_case("partial") {
            Response::Partial
        } else {
            Response::No
        }
    }
}

/// A row as stored in the database.
#[derive(Debug, Clone, PartialEq)]
pub struct Application {
    pub id: i64,
    pub professor_name: String,
    pub professor_email: String,
    pub university_name: String,
    pub program_name: String,
    pub country_name: String,
    /// `None` only for legacy rows whose stored date could not be parsed.
    pub application_last_date: Option<NaiveDate>,
    pub email_subject: String,
    pub email_body: String,
    pub email_send_date: Option<NaiveDate>,
    pub status: Status,
    pub response: Response,
}

impl Application {
    /// Turn a stored row into an editable draft.
    pub fn to_input(&self, today: NaiveDate) -> ApplicationInput {
        ApplicationInput {
            professor_name: self.professor_name.clone(),
            professor_email: self.professor_email.clone(),
            university_name: self.university_name.clone(),
            program_name: self.program_name.clone(),
            country_name: self.country_name.clone(),
            application_last_date: self.application_last_date.unwrap_or(today),
            email_subject: self.email_subject.clone(),
            email_body: self.email_body.clone(),
            email_send_date: self.email_send_date.unwrap_or(today),
            status: self.status,
            response: self.response,
        }
    }
}

/// What the add/edit form produces. Not trusted until validated.
#[derive(Debug, Clone, PartialEq)]
pub struct ApplicationInput {
    pub professor_name: String,
    pub professor_email: String,
    pub university_name: String,
    pub program_name: String,
    pub country_name: String,
    pub application_last_date: NaiveDate,
    pub email_subject: String,
    pub email_body: String,
    pub email_send_date: NaiveDate,
    pub status: Status,
    pub response: Response,
}

impl ApplicationInput {
    #[cfg(test)]
    pub fn blank(today: NaiveDate) -> Self {
        Self {
            professor_name: String::new(),
            professor_email: String::new(),
            university_name: String::new(),
            program_name: String::new(),
            country_name: String::new(),
            application_last_date: today,
            email_subject: String::new(),
            email_body: String::new(),
            email_send_date: today,
            status: Status::Pending,
            response: Response::No,
        }
    }

    /// Trim, check and return a cleaned copy, or every problem found.
    pub fn validate(&self) -> Result<ApplicationInput, Vec<String>> {
        let mut errors = Vec::new();
        let mut clean = self.clone();

        clean.professor_name = self.professor_name.trim().to_owned();
        clean.professor_email = self.professor_email.trim().to_owned();
        clean.university_name = self.university_name.trim().to_owned();
        clean.program_name = self.program_name.trim().to_owned();
        clean.country_name = self.country_name.trim().to_owned();
        clean.email_subject = self.email_subject.trim().to_owned();
        clean.email_body = self.email_body.trim().to_owned();

        for (label, value) in [
            ("Professor name", &clean.professor_name),
            ("Professor email", &clean.professor_email),
            ("University name", &clean.university_name),
            ("Program name", &clean.program_name),
            ("Country name", &clean.country_name),
        ] {
            if value.is_empty() {
                errors.push(format!("{label} is required."));
            }
        }

        for (label, value, max) in [
            ("Professor name", &clean.professor_name, MAX_SHORT_TEXT),
            ("University name", &clean.university_name, MAX_SHORT_TEXT),
            ("Program name", &clean.program_name, MAX_SHORT_TEXT),
            ("Country name", &clean.country_name, MAX_SHORT_TEXT),
            ("Email subject", &clean.email_subject, MAX_SUBJECT),
        ] {
            if value.chars().count() > max {
                errors.push(format!("{label} must be at most {max} characters."));
            }
            if value.chars().any(char::is_control) {
                errors.push(format!("{label} must be a single line without control characters."));
            }
        }

        if !clean.professor_email.is_empty() && !is_valid_email(&clean.professor_email) {
            errors.push("Professor email does not look like a valid address.".to_owned());
        }

        if clean.email_body.chars().count() > MAX_BODY {
            errors.push(format!("Email body must be at most {MAX_BODY} characters."));
        }
        if clean
            .email_body
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            errors.push("Email body contains unsupported control characters.".to_owned());
        }

        for (label, date) in [
            ("Application last date", clean.application_last_date),
            ("Email send date", clean.email_send_date),
        ] {
            if !(MIN_YEAR..=MAX_YEAR).contains(&date.year()) {
                errors.push(format!("{label} must be between {MIN_YEAR} and {MAX_YEAR}."));
            }
        }

        if errors.is_empty() {
            Ok(clean)
        } else {
            Err(errors)
        }
    }
}

/// Deliberately conservative address check. It rejects the obvious mistakes
/// (spaces, missing domain, double `@`) without trying to implement RFC 5322.
pub fn is_valid_email(s: &str) -> bool {
    if s.len() > MAX_EMAIL || s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let mut parts = s.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    if local.is_empty() || local.len() > 64 {
        return false;
    }
    if !domain.contains('.') {
        return false;
    }
    domain.split('.').all(|label| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_alphanumeric() || c == '-')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 12).unwrap()
    }

    fn valid_input() -> ApplicationInput {
        ApplicationInput {
            professor_name: "  Ada Lovelace ".into(),
            professor_email: "ada@cam.ac.uk".into(),
            university_name: "Cambridge".into(),
            program_name: "PhD Computing".into(),
            country_name: "UK".into(),
            ..ApplicationInput::blank(today())
        }
    }

    #[test]
    fn email_validation() {
        for ok in ["a@b.co", "first.last@dept.uni.edu", "x+tag@mail-server.org"] {
            assert!(is_valid_email(ok), "{ok}");
        }
        for bad in ["", "plain", "a@b", "@b.com", "a@@b.com", "a b@c.com", "a@-b.com", "a@b..com", "a@b.com\n"] {
            assert!(!is_valid_email(bad), "{bad:?}");
        }
    }

    #[test]
    fn validate_trims_and_accepts() {
        let clean = valid_input().validate().unwrap();
        assert_eq!(clean.professor_name, "Ada Lovelace");
    }

    #[test]
    fn validate_reports_every_problem() {
        let mut input = valid_input();
        input.professor_name = "   ".into();
        input.professor_email = "nope".into();
        input.email_subject = "line1\nline2".into();
        let errors = input.validate().unwrap_err();
        assert_eq!(errors.len(), 3, "{errors:?}");
    }

    #[test]
    fn validate_rejects_absurd_dates_and_long_text() {
        let mut input = valid_input();
        input.application_last_date = NaiveDate::from_ymd_opt(1850, 1, 1).unwrap();
        input.professor_name = "x".repeat(MAX_SHORT_TEXT + 1);
        assert_eq!(input.validate().unwrap_err().len(), 2);
    }

    #[test]
    fn legacy_values_parse_leniently() {
        assert_eq!(Status::from_db_str("DONE "), Status::Done);
        assert_eq!(Status::from_db_str("???"), Status::Pending);
        assert_eq!(Response::from_db_str("Partial"), Response::Partial);
        assert_eq!(Response::from_db_str(""), Response::No);
    }
}
