//! Search and filtering.
//!
//! The legacy app promised fuzzy search but only did SQL `LIKE`. Here every text
//! box is typo-tolerant: a query matches if it appears as a substring, or if some
//! window of the field is within a small Damerau-Levenshtein distance of it.

use strsim::damerau_levenshtein;

use crate::model::{Application, Response, Status};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filters {
    pub professor: String,
    pub email: String,
    pub university: String,
    pub program: String,
    pub country: String,
    pub status: Option<Status>,
    pub response: Option<Response>,
}

impl Filters {
    pub fn is_empty(&self) -> bool {
        *self == Filters::default()
    }

    pub fn clear(&mut self) {
        *self = Filters::default();
    }

    pub fn matches(&self, app: &Application) -> bool {
        fuzzy_contains(&app.professor_name, &self.professor)
            && fuzzy_contains(&app.professor_email, &self.email)
            && fuzzy_contains(&app.university_name, &self.university)
            && fuzzy_contains(&app.program_name, &self.program)
            && fuzzy_contains(&app.country_name, &self.country)
            && self.status.is_none_or(|s| s == app.status)
            && self.response.is_none_or(|r| r == app.response)
    }
}

/// Number of typos tolerated for a query of `len` characters.
/// Short queries must match exactly; otherwise nearly everything would match.
fn allowed_edits(len: usize) -> usize {
    match len {
        0..=3 => 0,
        4..=7 => 1,
        _ => 2,
    }
}

/// Case-insensitive "does `haystack` contain something close to `needle`?".
/// An empty needle matches everything.
pub fn fuzzy_contains(haystack: &str, needle: &str) -> bool {
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    let haystack = haystack.to_lowercase();
    if haystack.contains(&needle) {
        return true;
    }

    let needle_len = needle.chars().count();
    let allowed = allowed_edits(needle_len);
    if allowed == 0 {
        return false;
    }

    let hay: Vec<char> = haystack.chars().collect();
    let min_len = needle_len.saturating_sub(allowed).max(1);
    let max_len = needle_len + allowed;
    for start in 0..hay.len() {
        for len in min_len..=max_len {
            if start + len > hay.len() {
                break;
            }
            let window: String = hay[start..start + len].iter().collect();
            if damerau_levenshtein(&window, &needle) <= allowed {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn app(name: &str, uni: &str, status: Status, response: Response) -> Application {
        Application {
            id: 1,
            professor_name: name.into(),
            professor_email: format!("{}@example.edu", name.to_lowercase().replace(' ', ".")),
            university_name: uni.into(),
            program_name: "PhD".into(),
            country_name: "Germany".into(),
            application_last_date: NaiveDate::from_ymd_opt(2026, 12, 1),
            email_subject: String::new(),
            email_body: String::new(),
            email_send_date: None,
            status,
            response,
        }
    }

    #[test]
    fn substring_and_case_insensitive() {
        assert!(fuzzy_contains("Stanford University", "STANF"));
        assert!(fuzzy_contains("anything", ""));
        assert!(!fuzzy_contains("MIT", "oxford"));
    }

    #[test]
    fn tolerates_typos_in_longer_queries() {
        assert!(fuzzy_contains("Stanford University", "standford")); // inserted letter
        assert!(fuzzy_contains("Oxford", "oxfrod")); // transposition
        assert!(fuzzy_contains("Technische Universität München", "universtiy"));
        assert!(!fuzzy_contains("Stanford University", "harvard"));
    }

    #[test]
    fn short_queries_stay_exact() {
        assert!(!fuzzy_contains("abc", "abd"));
        assert!(fuzzy_contains("abcd", "abc"));
    }

    #[test]
    fn works_on_emails() {
        assert!(fuzzy_contains("jane.doe@cs.stanford.edu", "stanfrod"));
    }

    #[test]
    fn combined_filters() {
        let a = app("Jane Doe", "Stanford", Status::Pending, Response::No);
        let mut f = Filters { professor: "jane".into(), ..Filters::default() };
        assert!(f.matches(&a));
        f.status = Some(Status::Done);
        assert!(!f.matches(&a));
        f.status = Some(Status::Pending);
        f.response = Some(Response::Yes);
        assert!(!f.matches(&a));
        f.clear();
        assert!(f.is_empty() && f.matches(&a));
    }
}
