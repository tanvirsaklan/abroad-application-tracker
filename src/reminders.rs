//! Deadline reminders: classify each application by how close its deadline is.

use chrono::NaiveDate;

use crate::model::{Application, Status};

/// Pending applications due within this many days are flagged "due soon".
pub const SOON_WINDOW_DAYS: i64 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    /// Deadline passed; payload = days overdue (>= 1).
    Overdue(i64),
    DueToday,
    /// Payload = days remaining (1..=SOON_WINDOW_DAYS).
    DueSoon(i64),
    Later,
    /// Marked done, or the deadline is unknown: nothing to remind about.
    Closed,
}

impl Urgency {
    pub fn is_actionable(self) -> bool {
        matches!(self, Urgency::Overdue(_) | Urgency::DueToday | Urgency::DueSoon(_))
    }

    pub fn describe(self) -> Option<String> {
        match self {
            Urgency::Overdue(1) => Some("1 day overdue".to_owned()),
            Urgency::Overdue(d) => Some(format!("{d} days overdue")),
            Urgency::DueToday => Some("due today".to_owned()),
            Urgency::DueSoon(1) => Some("due tomorrow".to_owned()),
            Urgency::DueSoon(d) => Some(format!("due in {d} days")),
            Urgency::Later | Urgency::Closed => None,
        }
    }
}

pub fn urgency(app: &Application, today: NaiveDate) -> Urgency {
    let Some(deadline) = app.application_last_date else {
        return Urgency::Closed;
    };
    if app.status == Status::Done {
        return Urgency::Closed;
    }
    let days = (deadline - today).num_days();
    match days {
        d if d < 0 => Urgency::Overdue(-d),
        0 => Urgency::DueToday,
        d if d <= SOON_WINDOW_DAYS => Urgency::DueSoon(d),
        _ => Urgency::Later,
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReminderSummary {
    pub overdue: usize,
    pub due_today: usize,
    pub due_soon: usize,
}

impl ReminderSummary {
    pub fn total(&self) -> usize {
        self.overdue + self.due_today + self.due_soon
    }
}

pub fn summarize(apps: &[Application], today: NaiveDate) -> ReminderSummary {
    let mut s = ReminderSummary::default();
    for app in apps {
        match urgency(app, today) {
            Urgency::Overdue(_) => s.overdue += 1,
            Urgency::DueToday => s.due_today += 1,
            Urgency::DueSoon(_) => s.due_soon += 1,
            Urgency::Later | Urgency::Closed => {}
        }
    }
    s
}

/// Actionable applications, most urgent first (most overdue, then soonest).
pub fn upcoming(apps: &[Application], today: NaiveDate) -> Vec<(&Application, Urgency)> {
    let mut list: Vec<_> = apps
        .iter()
        .map(|a| (a, urgency(a, today)))
        .filter(|(_, u)| u.is_actionable())
        .collect();
    list.sort_by_key(|(a, _)| a.application_last_date);
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Response;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 12).unwrap()
    }

    fn app(id: i64, offset_days: i64, status: Status) -> Application {
        Application {
            id,
            professor_name: format!("P{id}"),
            professor_email: "p@u.edu".into(),
            university_name: "U".into(),
            program_name: "P".into(),
            country_name: "C".into(),
            application_last_date: Some(today() + chrono::Duration::days(offset_days)),
            email_subject: String::new(),
            email_body: String::new(),
            email_send_date: None,
            status,
            response: Response::No,
        }
    }

    #[test]
    fn classification() {
        assert_eq!(urgency(&app(1, -3, Status::Pending), today()), Urgency::Overdue(3));
        assert_eq!(urgency(&app(1, 0, Status::Pending), today()), Urgency::DueToday);
        assert_eq!(urgency(&app(1, 1, Status::Pending), today()), Urgency::DueSoon(1));
        assert_eq!(urgency(&app(1, 7, Status::Pending), today()), Urgency::DueSoon(7));
        assert_eq!(urgency(&app(1, 8, Status::Pending), today()), Urgency::Later);
        assert_eq!(urgency(&app(1, -3, Status::Done), today()), Urgency::Closed);
    }

    #[test]
    fn missing_deadline_is_closed() {
        let mut a = app(1, 0, Status::Pending);
        a.application_last_date = None;
        assert_eq!(urgency(&a, today()), Urgency::Closed);
    }

    #[test]
    fn summary_and_ordering() {
        let apps = vec![
            app(1, 5, Status::Pending),
            app(2, -2, Status::Pending),
            app(3, 0, Status::Pending),
            app(4, 30, Status::Pending),
            app(5, -9, Status::Done),
        ];
        let s = summarize(&apps, today());
        assert_eq!((s.overdue, s.due_today, s.due_soon, s.total()), (1, 1, 1, 3));
        let ids: Vec<i64> = upcoming(&apps, today()).iter().map(|(a, _)| a.id).collect();
        assert_eq!(ids, vec![2, 3, 1]);
    }
}
