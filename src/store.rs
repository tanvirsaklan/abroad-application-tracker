//! Application state shared by every component.
//!
//! [`Store`] is a bundle of Dioxus signals (all `Copy`), so any component can
//! grab it with `use_context` and call the action methods below. The rules
//! themselves (validation, search, reminders, export) live in the plain Rust
//! modules and know nothing about the UI.

use std::rc::Rc;

use chrono::NaiveDate;
use dioxus::prelude::*;

use crate::db::Database;
use crate::export;
use crate::model::{
    Application, ApplicationInput, Response, Status, DATE_FORMAT,
};
use crate::reminders::ReminderSummary;
use crate::search::Filters;
use crate::sort::Sort;

#[derive(Clone, PartialEq)]
pub enum Message {
    Info(String),
    Error(String),
}

/// What the add/edit form holds while the user is typing. Dates are plain
/// strings (as produced by `<input type="date">`) and are parsed on save.
#[derive(Clone, PartialEq)]
pub struct Form {
    pub professor_name: String,
    pub professor_email: String,
    pub university_name: String,
    pub program_name: String,
    pub country_name: String,
    pub last_date: String,
    pub email_subject: String,
    pub email_body: String,
    pub send_date: String,
    pub status: Status,
    pub response: Response,
}

fn fmt_date(d: NaiveDate) -> String {
    d.format(DATE_FORMAT).to_string()
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), DATE_FORMAT).ok()
}

impl Form {
    pub fn blank(today: NaiveDate) -> Self {
        Self {
            professor_name: String::new(),
            professor_email: String::new(),
            university_name: String::new(),
            program_name: String::new(),
            country_name: String::new(),
            last_date: fmt_date(today),
            email_subject: String::new(),
            email_body: String::new(),
            send_date: fmt_date(today),
            status: Status::Pending,
            response: Response::No,
        }
    }

    pub fn from_application(app: &Application, today: NaiveDate) -> Self {
        let input = app.to_input(today);
        Self {
            professor_name: input.professor_name,
            professor_email: input.professor_email,
            university_name: input.university_name,
            program_name: input.program_name,
            country_name: input.country_name,
            last_date: fmt_date(input.application_last_date),
            email_subject: input.email_subject,
            email_body: input.email_body,
            send_date: fmt_date(input.email_send_date),
            status: input.status,
            response: input.response,
        }
    }

    /// Parse the dates, then run the shared validation. Reports every problem.
    pub fn validate(&self, today: NaiveDate) -> Result<ApplicationInput, Vec<String>> {
        let mut errors = Vec::new();
        let mut date = |label: &str, raw: &str| match (raw.trim().is_empty(), parse_date(raw)) {
            (true, _) => {
                errors.push(format!("{label} is required."));
                today
            }
            (false, None) => {
                errors.push(format!("{label} is not a valid date."));
                today
            }
            (false, Some(d)) => d,
        };
        let application_last_date = date("Application last date", &self.last_date);
        let email_send_date = date("Email send date", &self.send_date);

        let input = ApplicationInput {
            professor_name: self.professor_name.clone(),
            professor_email: self.professor_email.clone(),
            university_name: self.university_name.clone(),
            program_name: self.program_name.clone(),
            country_name: self.country_name.clone(),
            application_last_date,
            email_subject: self.email_subject.clone(),
            email_body: self.email_body.clone(),
            email_send_date,
            status: self.status,
            response: self.response,
        };
        match input.validate() {
            Ok(clean) if errors.is_empty() => Ok(clean),
            Ok(_) => Err(errors),
            Err(mut more) => {
                errors.append(&mut more);
                Err(errors)
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct Editor {
    /// `None` = adding a new application.
    pub id: Option<i64>,
    pub form: Form,
    pub errors: Vec<String>,
}

#[derive(Clone, Copy)]
pub struct Store {
    pub db: Signal<Rc<Database>>,
    pub apps: Signal<Vec<Application>>,
    pub filters: Signal<Filters>,
    pub sort: Signal<Sort>,
    pub today: Signal<NaiveDate>,
    pub editor: Signal<Option<Editor>>,
    pub delete_target: Signal<Option<(i64, String)>>,
    pub show_reminders: Signal<bool>,
    pub message: Signal<Option<Message>>,
    pub export_open: Signal<bool>,
    pub hovered: Signal<Option<usize>>,
    /// Bumped when filters are cleared so the search boxes remount empty.
    pub filters_epoch: Signal<u32>,
    /// Filtered and sorted rows for the table.
    pub view: Memo<Vec<Application>>,
    pub country_counts: Memo<Vec<(String, usize)>>,
    pub response_counts: Memo<(usize, usize)>,
    pub summary: Memo<ReminderSummary>,
}

impl Store {
    pub fn info(mut self, text: impl Into<String>) {
        self.message.set(Some(Message::Info(text.into())));
    }

    pub fn error(mut self, text: impl Into<String>) {
        self.message.set(Some(Message::Error(text.into())));
    }

    pub fn clear_filters(mut self) {
        self.filters.write().clear();
        let next = self.filters_epoch.read().wrapping_add(1);
        self.filters_epoch.set(next);
    }

    pub fn reload(mut self) {
        let result = self.db.read().all();
        match result {
            Ok(apps) => self.apps.set(apps),
            Err(e) => self.error(format!("Could not load applications: {e}")),
        }
    }

    pub fn open_add(mut self) {
        let today = *self.today.read();
        self.editor.set(Some(Editor {
            id: None,
            form: Form::blank(today),
            errors: Vec::new(),
        }));
    }

    pub fn open_edit(mut self, id: i64) {
        let today = *self.today.read();
        let found = self.apps.read().iter().find(|a| a.id == id).cloned();
        if let Some(app) = found {
            self.editor.set(Some(Editor {
                id: Some(id),
                form: Form::from_application(&app, today),
                errors: Vec::new(),
            }));
        }
    }

    pub fn close_editor(mut self) {
        self.editor.set(None);
    }

    /// Apply a change to the open form (one keystroke, one dropdown, ...).
    pub fn edit_form(mut self, change: impl FnOnce(&mut Form)) {
        if let Some(editor) = self.editor.write().as_mut() {
            change(&mut editor.form);
        }
    }

    pub fn save_editor(mut self) {
        let Some(editor) = self.editor.read().clone() else { return };
        let today = *self.today.read();
        match editor.form.validate(today) {
            Err(errors) => {
                if let Some(ed) = self.editor.write().as_mut() {
                    ed.errors = errors;
                }
            }
            Ok(clean) => {
                let result = {
                    let db = self.db.read();
                    match editor.id {
                        Some(id) => db.update(id, &clean).map(|()| "Application updated."),
                        None => db.insert(&clean).map(|_| "Application added."),
                    }
                };
                match result {
                    Ok(text) => {
                        self.editor.set(None);
                        self.reload();
                        self.info(text);
                    }
                    Err(e) => {
                        let text = format!("Could not save: {e}");
                        if let Some(ed) = self.editor.write().as_mut() {
                            ed.errors = vec![text.clone()];
                        }
                        self.error(text);
                    }
                }
            }
        }
    }

    pub fn request_delete(mut self, id: i64) {
        let name = self
            .apps
            .read()
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.professor_name.clone());
        if let Some(name) = name {
            self.delete_target.set(Some((id, name)));
        }
    }

    pub fn confirm_delete(mut self) {
        let Some((id, _)) = self.delete_target.read().clone() else { return };
        self.delete_target.set(None);
        let result = self.db.read().delete(id);
        match result {
            Ok(()) => {
                self.reload();
                self.info("Application deleted.");
            }
            Err(e) => self.error(format!("Could not delete: {e}")),
        }
    }

    pub fn cancel_delete(mut self) {
        self.delete_target.set(None);
    }

    pub fn mark_done(self, id: i64) {
        let result = self.db.read().set_status(id, Status::Done);
        match result {
            Ok(()) => {
                self.reload();
                self.info("Marked as done.");
            }
            Err(e) => self.error(format!("Could not update application: {e}")),
        }
    }

    pub fn export_csv(self, everything: bool) {
        let all = self.apps.read().clone();
        let in_view = self.view.read().clone();
        let rows: Vec<&Application> = if everything {
            all.iter().collect()
        } else {
            in_view.iter().collect()
        };
        if rows.is_empty() {
            self.error("Nothing to export.");
            return;
        }
        let count = rows.len();
        match export::export_to_dir(&export::default_export_dir(), &rows) {
            Ok(path) => self.info(format!("Exported {count} applications to {}", path.display())),
            Err(e) => self.error(format!("Export failed: {e}")),
        }
    }
}
