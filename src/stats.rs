//! Aggregates for the chart and the "Response (yes/total)" header.

use std::collections::BTreeMap;

use crate::model::{Application, Response};

/// Applications per country, largest first. Countries are grouped
/// case-insensitively ("canada" and "Canada" are the same slice).
pub fn country_counts(apps: &[Application]) -> Vec<(String, usize)> {
    let mut groups: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for app in apps {
        let display = app.country_name.trim();
        let display = if display.is_empty() { "(unknown)" } else { display };
        let entry = groups
            .entry(display.to_lowercase())
            .or_insert_with(|| (display.to_owned(), 0));
        entry.1 += 1;
    }
    let mut counts: Vec<(String, usize)> = groups.into_values().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counts
}

/// `(yes_count, total)` across all applications.
pub fn response_counts(apps: &[Application]) -> (usize, usize) {
    let yes = apps.iter().filter(|a| a.response == Response::Yes).count();
    (yes, apps.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;

    fn app(country: &str, response: Response) -> Application {
        Application {
            id: 0,
            professor_name: String::new(),
            professor_email: String::new(),
            university_name: String::new(),
            program_name: String::new(),
            country_name: country.into(),
            application_last_date: None,
            email_subject: String::new(),
            email_body: String::new(),
            email_send_date: None,
            status: Status::Pending,
            response,
        }
    }

    #[test]
    fn groups_case_insensitively_and_orders_by_count() {
        let apps = vec![
            app("Canada", Response::No),
            app("germany", Response::Yes),
            app("canada", Response::Partial),
            app("Germany", Response::Yes),
            app("Canada", Response::No),
            app("", Response::No),
        ];
        assert_eq!(
            country_counts(&apps),
            vec![
                ("Canada".to_owned(), 3),
                ("germany".to_owned(), 2),
                ("(unknown)".to_owned(), 1)
            ]
        );
        assert_eq!(response_counts(&apps), (2, 6));
    }
}
