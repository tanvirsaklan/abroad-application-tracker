//! Column sorting for the main table.

use std::cmp::Ordering;

use chrono::NaiveDate;

use crate::model::Application;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Professor,
    Email,
    University,
    Program,
    Country,
    Deadline,
    Subject,
    SendDate,
    Status,
    Response,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub key: SortKey,
    pub ascending: bool,
}

impl Default for Sort {
    /// Soonest deadline first: the most useful default for a tracker.
    fn default() -> Self {
        Self { key: SortKey::Deadline, ascending: true }
    }
}

impl Sort {
    /// Clicking the active column flips direction; a new column starts ascending.
    pub fn toggle(&mut self, key: SortKey) {
        if self.key == key {
            self.ascending = !self.ascending;
        } else {
            self.key = key;
            self.ascending = true;
        }
    }

    pub fn compare(&self, a: &Application, b: &Application) -> Ordering {
        let ord = match self.key {
            SortKey::Professor => text_cmp(&a.professor_name, &b.professor_name),
            SortKey::Email => text_cmp(&a.professor_email, &b.professor_email),
            SortKey::University => text_cmp(&a.university_name, &b.university_name),
            SortKey::Program => text_cmp(&a.program_name, &b.program_name),
            SortKey::Country => text_cmp(&a.country_name, &b.country_name),
            SortKey::Deadline => date_cmp(a.application_last_date, b.application_last_date),
            SortKey::Subject => text_cmp(&a.email_subject, &b.email_subject),
            SortKey::SendDate => date_cmp(a.email_send_date, b.email_send_date),
            SortKey::Status => a.status.cmp(&b.status),
            SortKey::Response => a.response.cmp(&b.response),
        };
        let ord = if self.ascending { ord } else { ord.reverse() };
        // Stable, deterministic order for equal keys.
        ord.then_with(|| a.id.cmp(&b.id))
    }
}

fn text_cmp(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

/// Unknown dates sort as "latest".
fn date_cmp(a: Option<NaiveDate>, b: Option<NaiveDate>) -> Ordering {
    a.unwrap_or(NaiveDate::MAX).cmp(&b.unwrap_or(NaiveDate::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Response, Status};

    fn app(id: i64, name: &str, day: Option<u32>) -> Application {
        Application {
            id,
            professor_name: name.into(),
            professor_email: String::new(),
            university_name: String::new(),
            program_name: String::new(),
            country_name: String::new(),
            application_last_date: day.and_then(|d| NaiveDate::from_ymd_opt(2026, 12, d)),
            email_subject: String::new(),
            email_body: String::new(),
            email_send_date: None,
            status: Status::Pending,
            response: Response::No,
        }
    }

    #[test]
    fn default_sorts_by_deadline_with_unknown_last() {
        let mut v = vec![app(1, "a", None), app(2, "b", Some(20)), app(3, "c", Some(5))];
        let sort = Sort::default();
        v.sort_by(|x, y| sort.compare(x, y));
        let ids: Vec<i64> = v.iter().map(|a| a.id).collect();
        assert_eq!(ids, vec![3, 2, 1]);
    }

    #[test]
    fn toggle_flips_then_resets() {
        let mut s = Sort::default();
        s.toggle(SortKey::Deadline);
        assert!(!s.ascending);
        s.toggle(SortKey::Professor);
        assert_eq!((s.key, s.ascending), (SortKey::Professor, true));
    }

    #[test]
    fn text_sort_is_case_insensitive() {
        let mut v = vec![app(1, "bob", None), app(2, "Alice", None)];
        let sort = Sort { key: SortKey::Professor, ascending: true };
        v.sort_by(|x, y| sort.compare(x, y));
        assert_eq!(v[0].professor_name, "Alice");
    }
}
