//! Dioxus components. Pure presentation: every rule and every database call
//! lives in `store.rs` and the logic modules.

use std::f32::consts::{PI, TAU};
use std::rc::Rc;
use std::time::Duration;

use chrono::{Local, NaiveDate};
use dioxus::prelude::*;

use crate::db::{self, Database};
use crate::model::{
    Application, Response, Status, DATE_FORMAT, MAX_BODY, MAX_EMAIL, MAX_SHORT_TEXT, MAX_SUBJECT,
};
use crate::reminders::{self, Urgency, SOON_WINDOW_DAYS};
use crate::search::Filters;
use crate::sort::{Sort, SortKey};
use crate::stats;
use crate::store::{Message, Store};

pub const APP_TITLE: &str = "Abroad Application Tracker";

const PIE_COLORS: [&str; 10] = [
    "#6366f1", "#f43f5e", "#10b981", "#f59e0b", "#8b5cf6", "#0ea5e9", "#f97316", "#14b8a6",
    "#ec4899", "#84cc16",
];

/// Newtype so a shared database handle can be a component prop.
#[derive(Clone)]
struct DbHandle(Rc<Database>);

impl PartialEq for DbHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

// ---------------------------------------------------------------- root

#[allow(non_snake_case)]
pub fn App() -> Element {
    let opened = use_hook(|| {
        let path = db::default_db_path();
        let result = Database::open(&path).map(Rc::new).map_err(|e| e.to_string());
        (path, result)
    });
    match opened {
        (path, Ok(db)) => rsx! {
            Main { db: DbHandle(db.clone()), path: path.display().to_string() }
        },
        (path, Err(err)) => rsx! {
            div { class: "fatal",
                div { class: "card",
                    h1 { "Could not start" }
                    p { class: "err", "Could not open the database at {path.display()}: {err}" }
                    p { class: "muted",
                        "Check that the data folder is writable, or point PROFESSOR_TRACKER_DB at a database file you can write to."
                    }
                }
            }
        },
    }
}

#[component]
fn Main(db: DbHandle, path: String) -> Element {
    let initial = use_hook(|| {
        let today = Local::now().date_naive();
        match db.0.all() {
            Ok(apps) => (apps, None, today),
            Err(e) => (Vec::new(), Some(Message::Error(format!("Could not load applications: {e}"))), today),
        }
    });
    let (apps0, message0, today0) = initial;

    let db_sig = use_signal(|| db.0.clone());
    let apps = use_signal(|| apps0.clone());
    let filters = use_signal(Filters::default);
    let sort = use_signal(Sort::default);
    let mut today = use_signal(|| today0);
    let editor = use_signal(|| None);
    let delete_target = use_signal(|| None);
    let message = use_signal(|| message0.clone());
    let export_open = use_signal(|| false);
    let hovered = use_signal(|| None);
    let filters_epoch = use_signal(|| 0_u32);

    let view = use_memo(move || {
        let filters = filters.read();
        let sort = *sort.read();
        let mut rows: Vec<Application> =
            apps.read().iter().filter(|a| filters.matches(a)).cloned().collect();
        rows.sort_by(|a, b| sort.compare(a, b));
        rows
    });
    let country_counts = use_memo(move || stats::country_counts(&apps.read()));
    let response_counts = use_memo(move || stats::response_counts(&apps.read()));
    let summary = use_memo(move || reminders::summarize(&apps.read(), *today.read()));

    // Reminders open on launch when something needs attention.
    let show_reminders = use_signal(|| reminders::summarize(&apps0, today0).total() > 0);

    use_context_provider(|| Store {
        db: db_sig,
        apps,
        filters,
        sort,
        today,
        editor,
        delete_target,
        show_reminders,
        message,
        export_open,
        hovered,
        filters_epoch,
        view,
        country_counts,
        response_counts,
        summary,
    });

    // Keeps "today" correct if the window stays open past midnight.
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let now = Local::now().date_naive();
            if *today.peek() != now {
                today.set(now);
            }
        }
    });

    rsx! {
        div { class: "app",
            Topbar {}
            Toolbar {}
            TableCard {}
            ChartCard {}
            StatusBar { path }
            RemindersModal {}
            EditorModal {}
            DeleteModal {}
        }
    }
}

// ------------------------------------------------------------- top bar

#[component]
fn Topbar() -> Element {
    let store = use_context::<Store>();
    let summary = *store.summary.read();
    let mut parts = Vec::new();
    if summary.overdue > 0 {
        parts.push(format!("{} overdue", summary.overdue));
    }
    if summary.due_today > 0 {
        parts.push(format!("{} due today", summary.due_today));
    }
    if summary.due_soon > 0 {
        parts.push(format!("{} due within {SOON_WINDOW_DAYS} days", summary.due_soon));
    }
    let banner_text = parts.join("   ·   ");
    let banner_class = if summary.overdue > 0 { "banner" } else { "banner soft" };
    rsx! {
        header { class: "topbar",
            div { class: "logo",
                svg { view_box: "0 0 24 24",
                    path { d: "M12 3 1 9l11 6 9-4.91V17h2V9L12 3zM5 13.18v4L12 21l7-3.82v-4L12 17l-7-3.82z" }
                }
            }
            div {
                h1 { "{APP_TITLE}" }
                p { "Keep track of every professor you have written to, and every deadline." }
            }
            if summary.total() > 0 {
                div { class: "{banner_class}",
                    span { class: "dot" }
                    "Deadlines:  {banner_text}"
                }
            }
        }
    }
}

// ------------------------------------------------------------- toolbar

#[component]
fn SearchBox(placeholder: &'static str, value: String, oninput: EventHandler<String>) -> Element {
    rsx! {
        input {
            class: "search",
            r#type: "text",
            placeholder,
            maxlength: "{MAX_SHORT_TEXT}",
            initial_value: "{value}",
            oninput: move |e| oninput.call(e.value()),
        }
    }
}

#[component]
fn Toolbar() -> Element {
    let mut store = use_context::<Store>();
    let filters = store.filters.read().clone();
    let summary = *store.summary.read();
    let epoch = *store.filters_epoch.read();

    let status_value = filters.status.map_or("any", Status::as_db_str);
    let response_value = filters.response.map_or("any", Response::as_db_str);

    let pill_class = if summary.overdue > 0 { "count-pill" } else { "count-pill warn" };
    let reminder_total = summary.total();

    rsx! {
        section { class: "toolbar card",
            // A keyed one-item loop: changing the key remounts the boxes, which
            // is how "Clear filters" empties the text the user typed.
            for e in std::iter::once(epoch) {
                div { class: "search-row", key: "{e}",
                SearchBox {
                    placeholder: "Search professor name",
                    value: filters.professor.clone(),
                    oninput: move |v| store.filters.write().professor = v,
                }
                SearchBox {
                    placeholder: "Search professor email",
                    value: filters.email.clone(),
                    oninput: move |v| store.filters.write().email = v,
                }
                SearchBox {
                    placeholder: "Search university",
                    value: filters.university.clone(),
                    oninput: move |v| store.filters.write().university = v,
                }
                SearchBox {
                    placeholder: "Search program",
                    value: filters.program.clone(),
                    oninput: move |v| store.filters.write().program = v,
                }
                SearchBox {
                    placeholder: "Search country",
                    value: filters.country.clone(),
                    oninput: move |v| store.filters.write().country = v,
                }
            }
            }
            div { class: "filter-row",
                label { class: "select-field",
                    "Status"
                    select {
                        value: "{status_value}",
                        onchange: move |e| {
                            let v = e.value();
                            store.filters.write().status = if v == "any" { None } else { Some(Status::from_db_str(&v)) };
                        },
                        option { value: "any", selected: filters.status.is_none(), "Any" }
                        for s in Status::ALL {
                            option { value: s.as_db_str(), selected: filters.status == Some(s), "{s.label()}" }
                        }
                    }
                }
                label { class: "select-field",
                    "Response"
                    select {
                        value: "{response_value}",
                        onchange: move |e| {
                            let v = e.value();
                            store.filters.write().response = if v == "any" { None } else { Some(Response::from_db_str(&v)) };
                        },
                        option { value: "any", selected: filters.response.is_none(), "Any" }
                        for r in Response::ALL {
                            option { value: r.as_db_str(), selected: filters.response == Some(r), "{r.label()}" }
                        }
                    }
                }
                button {
                    class: "btn btn-ghost",
                    disabled: filters.is_empty(),
                    onclick: move |_| store.clear_filters(),
                    "Clear filters"
                }
                div { class: "spacer" }
                button { class: "btn btn-primary", onclick: move |_| store.open_add(),
                    "+  Add application"
                }
                button {
                    class: "btn btn-secondary",
                    onclick: move |_| {
                        let open = *store.show_reminders.read();
                        store.show_reminders.set(!open);
                    },
                    "Reminders"
                    if reminder_total > 0 {
                        span { class: "{pill_class}", "{reminder_total}" }
                    }
                }
                ExportMenu {}
            }
        }
    }
}

#[component]
fn ExportMenu() -> Element {
    let mut store = use_context::<Store>();
    let open = *store.export_open.read();
    let in_view = store.view.read().len();
    let total = store.apps.read().len();
    rsx! {
        div { class: "menu-wrap",
            button {
                class: "btn btn-secondary",
                onclick: move |_| {
                    let now = *store.export_open.read();
                    store.export_open.set(!now);
                },
                "Export CSV"
            }
            if open {
                div { class: "menu-backdrop", onclick: move |_| store.export_open.set(false) }
                div { class: "menu",
                    button {
                        onclick: move |_| {
                            store.export_open.set(false);
                            store.export_csv(false);
                        },
                        "Current view"
                        small { "{in_view} rows" }
                    }
                    button {
                        onclick: move |_| {
                            store.export_open.set(false);
                            store.export_csv(true);
                        },
                        "All applications"
                        small { "{total} rows" }
                    }
                }
            }
        }
    }
}

// --------------------------------------------------------------- table

#[component]
fn SortTh(label: String, column: SortKey) -> Element {
    let mut store = use_context::<Store>();
    let sort = *store.sort.read();
    let active = sort.key == column;
    let class = if active { "sortable active" } else { "sortable" };
    let arrow = match (active, sort.ascending) {
        (false, _) => "",
        (true, true) => "▲",
        (true, false) => "▼",
    };
    rsx! {
        th { class: "{class}", onclick: move |_| store.sort.write().toggle(column),
            "{label}"
            span { class: "arrow", "{arrow}" }
        }
    }
}

#[component]
fn TableCard() -> Element {
    let store = use_context::<Store>();
    let view = store.view.read().clone();
    let total = store.apps.read().len();
    let (yes, all) = *store.response_counts.read();
    let response_label = format!("Response ({yes}/{all})");
    let shown = view.len();

    rsx! {
        section { class: "table-card card",
            div { class: "card-head",
                h2 { "Applications" }
                span { class: "muted", "Showing {shown} of {total} applications" }
            }
            div { class: "table-scroll",
                table { class: "tracker",
                    thead {
                        tr {
                            SortTh { label: "Professor", column: SortKey::Professor }
                            SortTh { label: "Email", column: SortKey::Email }
                            SortTh { label: "University", column: SortKey::University }
                            SortTh { label: "Program", column: SortKey::Program }
                            SortTh { label: "Country", column: SortKey::Country }
                            SortTh { label: "Application deadline", column: SortKey::Deadline }
                            SortTh { label: "Email subject", column: SortKey::Subject }
                            th { "Email body" }
                            SortTh { label: "Sent on", column: SortKey::SendDate }
                            SortTh { label: "Status", column: SortKey::Status }
                            SortTh { label: "{response_label}", column: SortKey::Response }
                            th { "Actions" }
                        }
                    }
                    tbody {
                        for app in view {
                            Row { key: "{app.id}", app }
                        }
                        if shown == 0 {
                            tr {
                                td { class: "empty", colspan: "12", "No applications to show." }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn fmt_date(date: Option<NaiveDate>) -> String {
    date.map(|d| d.format(DATE_FORMAT).to_string()).unwrap_or_default()
}

fn urgency_class(urgency: Urgency) -> &'static str {
    match urgency {
        Urgency::Overdue(_) => "overdue",
        Urgency::DueToday => "today",
        Urgency::DueSoon(_) => "soon",
        Urgency::Later | Urgency::Closed => "",
    }
}

#[component]
fn Row(app: Application) -> Element {
    let store = use_context::<Store>();
    let today = *store.today.read();
    let urgency = reminders::urgency(&app, today);
    let note = urgency.describe().unwrap_or_default();
    let note_class = format!("pill {}", urgency_class(urgency));
    let deadline = app
        .application_last_date
        .map(|d| d.format(DATE_FORMAT).to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    let sent = fmt_date(app.email_send_date);
    let one_line = app.email_body.split_whitespace().collect::<Vec<_>>().join(" ");
    let status_class = match app.status {
        Status::Pending => "pill pending",
        Status::Done => "pill done",
    };
    let response_class = match app.response {
        Response::Yes => "pill r-yes",
        Response::Partial => "pill r-partial",
        Response::No => "pill r-no",
    };
    let id = app.id;
    let pending = app.status == Status::Pending;

    rsx! {
        tr {
            td { class: "cell strong", title: "{app.professor_name}", "{app.professor_name}" }
            td { class: "cell dim", title: "{app.professor_email}", "{app.professor_email}" }
            td { class: "cell", title: "{app.university_name}", "{app.university_name}" }
            td { class: "cell", title: "{app.program_name}", "{app.program_name}" }
            td { class: "cell", title: "{app.country_name}", "{app.country_name}" }
            td {
                div { class: "deadline-cell",
                    span { class: "date", "{deadline}" }
                    if !note.is_empty() {
                        span { class: "{note_class}", "{note}" }
                    }
                }
            }
            td { class: "cell", title: "{app.email_subject}", "{app.email_subject}" }
            td { class: "cell wide dim", title: "{app.email_body}", "{one_line}" }
            td { class: "dim", "{sent}" }
            td {
                span { class: "{status_class}", "{app.status.label()}" }
            }
            td {
                span { class: "{response_class}", "{app.response.label()}" }
            }
            td {
                div { class: "actions",
                    button {
                        class: "btn btn-sm btn-edit",
                        onclick: move |_| store.open_edit(id),
                        "Edit"
                    }
                    button {
                        class: "btn btn-sm btn-done",
                        disabled: !pending,
                        onclick: move |_| store.mark_done(id),
                        "Mark done"
                    }
                    button {
                        class: "btn btn-sm btn-del",
                        onclick: move |_| store.request_delete(id),
                        "Delete"
                    }
                }
            }
        }
    }
}

// --------------------------------------------------------------- chart

#[derive(Clone, PartialEq)]
struct SliceVm {
    index: usize,
    name: String,
    count: usize,
    percent: f32,
    color: &'static str,
    path: String,
}

const CENTER: f32 = 100.0;
const RADIUS: f32 = 92.0;

fn point(angle: f32) -> (f32, f32) {
    (CENTER + RADIUS * angle.cos(), CENTER + RADIUS * angle.sin())
}

fn slice_path(start: f32, end: f32) -> String {
    let (x1, y1) = point(start);
    let (x2, y2) = point(end);
    let large = if end - start > PI { 1 } else { 0 };
    format!(
        "M {CENTER} {CENTER} L {x1:.3} {y1:.3} A {RADIUS} {RADIUS} 0 {large} 1 {x2:.3} {y2:.3} Z"
    )
}

fn build_slices(counts: &[(String, usize)]) -> Vec<SliceVm> {
    let total: usize = counts.iter().map(|(_, c)| *c).sum();
    let mut start = -PI / 2.0; // 12 o'clock
    counts
        .iter()
        .enumerate()
        .map(|(index, (name, count))| {
            let fraction = *count as f32 / total as f32;
            let end = start + TAU * fraction;
            let vm = SliceVm {
                index,
                name: name.clone(),
                count: *count,
                percent: fraction * 100.0,
                color: PIE_COLORS[index % PIE_COLORS.len()],
                path: slice_path(start, end),
            };
            start = end;
            vm
        })
        .collect()
}

fn highlight_class(base: &str, hovered: Option<usize>, index: usize) -> String {
    match hovered {
        Some(h) if h == index => format!("{base} active"),
        Some(_) => format!("{base} dim"),
        None => base.to_owned(),
    }
}

#[component]
fn PieSlice(vm: SliceVm, single: bool) -> Element {
    let mut store = use_context::<Store>();
    let hovered = *store.hovered.read();
    let class = highlight_class("slice", hovered, vm.index);
    let index = vm.index;
    if single {
        rsx! {
            circle {
                class: "{class}",
                cx: "{CENTER}",
                cy: "{CENTER}",
                r: "{RADIUS}",
                fill: "{vm.color}",
                onmouseenter: move |_| store.hovered.set(Some(index)),
                onmouseleave: move |_| store.hovered.set(None),
            }
        }
    } else {
        rsx! {
            path {
                class: "{class}",
                d: "{vm.path}",
                fill: "{vm.color}",
                onmouseenter: move |_| store.hovered.set(Some(index)),
                onmouseleave: move |_| store.hovered.set(None),
            }
        }
    }
}

#[component]
fn LegendItem(vm: SliceVm) -> Element {
    let mut store = use_context::<Store>();
    let hovered = *store.hovered.read();
    let class = highlight_class("", hovered, vm.index);
    let index = vm.index;
    rsx! {
        li {
            class: "{class}",
            onmouseenter: move |_| store.hovered.set(Some(index)),
            onmouseleave: move |_| store.hovered.set(None),
            span { class: "swatch", style: "background: {vm.color}" }
            span { class: "name", "{vm.name}" }
            span { class: "num", "{vm.count}" }
            span { class: "pct", "{vm.percent:.0}%" }
        }
    }
}

#[component]
fn ChartCard() -> Element {
    let store = use_context::<Store>();
    let counts = store.country_counts.read().clone();
    let slices = build_slices(&counts);
    let total: usize = counts.iter().map(|(_, c)| *c).sum();
    let single = slices.len() == 1;
    let hovered = *store.hovered.read();
    let caption = hovered
        .and_then(|i| slices.get(i))
        .map(|s| format!("{}: {} of {total}", s.name, s.count))
        .unwrap_or_default();

    rsx! {
        section { class: "chart-card card",
            div { class: "card-head",
                h2 { "Applications by country" }
            }
            if total == 0 {
                div { class: "chart-body",
                    p { class: "no-data", "No applications yet. Add one to see the chart." }
                }
            } else {
                div { class: "chart-body",
                    div { class: "pie-wrap",
                        svg { class: "pie", view_box: "0 0 200 200",
                            for vm in slices.clone() {
                                PieSlice { key: "{vm.index}", vm, single }
                            }
                        }
                        div { class: "pie-caption", "{caption}" }
                    }
                    ul { class: "legend",
                        for vm in slices {
                            LegendItem { key: "{vm.index}", vm }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------- status bar

#[component]
fn StatusBar(path: String) -> Element {
    let store = use_context::<Store>();
    let message = store.message.read().clone();
    rsx! {
        footer { class: "statusbar",
            match message {
                Some(Message::Info(text)) => rsx! { span { class: "msg info", "{text}" } },
                Some(Message::Error(text)) => rsx! { span { class: "msg error", "{text}" } },
                None => rsx! { span { class: "msg", "Ready" } },
            }
            span { class: "path", "Database: {path}" }
        }
    }
}

// -------------------------------------------------------------- dialogs

#[component]
fn TextField(
    label: &'static str,
    required: bool,
    value: String,
    max: usize,
    class: &'static str,
    oninput: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "field {class}",
            label {
                "{label}"
                if required {
                    span { class: "req", "*" }
                }
            }
            input {
                r#type: "text",
                maxlength: "{max}",
                initial_value: "{value}",
                oninput: move |e| oninput.call(e.value()),
            }
        }
    }
}

#[component]
fn DateField(
    label: &'static str,
    required: bool,
    value: String,
    class: &'static str,
    oninput: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "field {class}",
            label {
                "{label}"
                if required {
                    span { class: "req", "*" }
                }
            }
            input {
                r#type: "date",
                min: "2000-01-01",
                max: "2100-12-31",
                initial_value: "{value}",
                oninput: move |e| oninput.call(e.value()),
            }
        }
    }
}

#[component]
fn EditorModal() -> Element {
    let store = use_context::<Store>();
    let Some(editor) = store.editor.read().clone() else {
        return rsx! {};
    };
    let form = editor.form;
    let title = if editor.id.is_some() { "Edit application" } else { "Add application" };

    rsx! {
        div { class: "overlay",
            div { class: "modal", role: "dialog",
                div { class: "modal-head",
                    h3 { "{title}" }
                    button { class: "icon-btn", onclick: move |_| store.close_editor(), "✕" }
                }
                div { class: "modal-body",
                    div { class: "form",
                        TextField {
                            label: "Professor name",
                            required: true,
                            class: "",
                            max: MAX_SHORT_TEXT,
                            value: form.professor_name.clone(),
                            oninput: move |v: String| store.edit_form(|f| f.professor_name = v),
                        }
                        TextField {
                            label: "Professor email",
                            required: true,
                            class: "",
                            max: MAX_EMAIL,
                            value: form.professor_email.clone(),
                            oninput: move |v: String| store.edit_form(|f| f.professor_email = v),
                        }
                        TextField {
                            label: "University name",
                            required: true,
                            class: "",
                            max: MAX_SHORT_TEXT,
                            value: form.university_name.clone(),
                            oninput: move |v: String| store.edit_form(|f| f.university_name = v),
                        }
                        TextField {
                            label: "Program name",
                            required: true,
                            class: "",
                            max: MAX_SHORT_TEXT,
                            value: form.program_name.clone(),
                            oninput: move |v: String| store.edit_form(|f| f.program_name = v),
                        }
                        TextField {
                            label: "Country name",
                            required: true,
                            class: "",
                            max: MAX_SHORT_TEXT,
                            value: form.country_name.clone(),
                            oninput: move |v: String| store.edit_form(|f| f.country_name = v),
                        }
                        DateField {
                            label: "Application last date",
                            required: true,
                            class: "",
                            value: form.last_date.clone(),
                            oninput: move |v: String| store.edit_form(|f| f.last_date = v),
                        }
                        TextField {
                            label: "Email subject",
                            required: false,
                            class: "full",
                            max: MAX_SUBJECT,
                            value: form.email_subject.clone(),
                            oninput: move |v: String| store.edit_form(|f| f.email_subject = v),
                        }
                        div { class: "field full",
                            label { "Email body" }
                            textarea {
                                maxlength: "{MAX_BODY}",
                                initial_value: "{form.email_body}",
                                oninput: move |e| {
                                    let v = e.value();
                                    store.edit_form(|f| f.email_body = v);
                                },
                            }
                        }
                        DateField {
                            label: "Email send date",
                            required: false,
                            class: "third",
                            value: form.send_date.clone(),
                            oninput: move |v: String| store.edit_form(|f| f.send_date = v),
                        }
                        div { class: "field third",
                            label { "Status" }
                            select {
                                value: "{form.status.as_db_str()}",
                                onchange: move |e| {
                                    let v = e.value();
                                    store.edit_form(|f| f.status = Status::from_db_str(&v));
                                },
                                for s in Status::ALL {
                                    option { value: s.as_db_str(), selected: form.status == s, "{s.label()}" }
                                }
                            }
                        }
                        div { class: "field third",
                            label { "Response" }
                            select {
                                value: "{form.response.as_db_str()}",
                                onchange: move |e| {
                                    let v = e.value();
                                    store.edit_form(|f| f.response = Response::from_db_str(&v));
                                },
                                for r in Response::ALL {
                                    option { value: r.as_db_str(), selected: form.response == r, "{r.label()}" }
                                }
                            }
                        }
                    }
                    if !editor.errors.is_empty() {
                        ul { class: "errors",
                            for error in editor.errors.iter() {
                                li { "{error}" }
                            }
                        }
                    }
                }
                div { class: "modal-foot",
                    button { class: "btn btn-secondary", onclick: move |_| store.close_editor(), "Cancel" }
                    button { class: "btn btn-primary", onclick: move |_| store.save_editor(), "Save" }
                }
            }
        }
    }
}

#[component]
fn DeleteModal() -> Element {
    let store = use_context::<Store>();
    let Some((_, name)) = store.delete_target.read().clone() else {
        return rsx! {};
    };
    rsx! {
        div { class: "overlay",
            div { class: "modal small", role: "alertdialog",
                div { class: "modal-head",
                    h3 { "Confirm delete" }
                }
                div { class: "modal-body",
                    p { class: "modal-text", "Delete the application for {name}?" }
                    p { class: "modal-text muted", "This cannot be undone." }
                }
                div { class: "modal-foot",
                    button { class: "btn btn-secondary", onclick: move |_| store.cancel_delete(), "Cancel" }
                    button { class: "btn btn-danger", onclick: move |_| store.confirm_delete(), "Delete" }
                }
            }
        }
    }
}

#[component]
fn ReminderItem(app: Application, urgency: Urgency) -> Element {
    let store = use_context::<Store>();
    let id = app.id;
    let class = format!("rem-item {}", urgency_class(urgency));
    let pill = format!("pill {}", urgency_class(urgency));
    let note = urgency.describe().unwrap_or_default();
    let deadline = fmt_date(app.application_last_date);
    rsx! {
        div { class: "{class}",
            div { class: "rem-top",
                span { class: "{pill}", "{note}" }
                strong { "{app.professor_name}" }
            }
            div { class: "rem-meta",
                "{app.university_name} · {app.program_name}  —  deadline {deadline}"
            }
            div { class: "rem-actions",
                button { class: "btn btn-sm btn-edit", onclick: move |_| store.open_edit(id), "Edit" }
                button { class: "btn btn-sm btn-done", onclick: move |_| store.mark_done(id), "Mark done" }
            }
        }
    }
}

#[component]
fn RemindersModal() -> Element {
    let mut store = use_context::<Store>();
    if !*store.show_reminders.read() {
        return rsx! {};
    }
    let today = *store.today.read();
    let list: Vec<(Application, Urgency)> = {
        let apps = store.apps.read();
        reminders::upcoming(&apps, today)
            .into_iter()
            .map(|(a, u)| (a.clone(), u))
            .collect()
    };
    rsx! {
        div { class: "overlay",
            div { class: "modal", role: "dialog",
                div { class: "modal-head",
                    h3 { "Deadline reminders" }
                    button { class: "icon-btn", onclick: move |_| store.show_reminders.set(false), "✕" }
                }
                div { class: "modal-body",
                    if list.is_empty() {
                        p { class: "rem-empty",
                            "Nothing overdue or due within the next {SOON_WINDOW_DAYS} days."
                        }
                    } else {
                        p { class: "modal-text",
                            "Pending applications with a deadline that needs attention:"
                        }
                        div { class: "rem-list",
                            for (app, urgency) in list {
                                ReminderItem { key: "{app.id}", app, urgency }
                            }
                        }
                    }
                }
                div { class: "modal-foot",
                    button { class: "btn btn-secondary", onclick: move |_| store.show_reminders.set(false), "Close" }
                }
            }
        }
    }
}
