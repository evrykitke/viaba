//! The frame a report is read in.
//!
//! A page inside the shell, not a takeover of it: the navigation and the top
//! bar are untouched, and what changes is that the content area holds one
//! report rather than a panel with something else beside it.
//!
//! The toolbar is the frame's, so that twenty reports do not each grow their
//! own controls. It carries the report's own parameters - a customer, a span -
//! on the left, and the report's actions on the right. **Page navigation
//! belongs here and is not built**: nothing counts pages until the paginator
//! lands, and a control that always said "1 of 1" would be a promise.

use leptos::html;
use leptos::prelude::*;
use leptos_router::components::A;

use std::time::Duration;

use phonix_core::report::{ExportState, PageSetup};
use serde_json::Map;
use serde_json::Value as Parameters;
use uuid::Uuid;

use super::{ExportFormat, Extent, ReportDefinition};
use crate::icons::{Icon, IconSize};
use crate::l;
use crate::server_fns::file_fns as content;
use crate::server_fns::report_fns::{export_state, raise_export, write_now};
use crate::ui::alert::{Alert, Alerts};
use crate::ui::table::export;
use crate::ui::viewer::Viewer;

/// How many CSS pixels one millimetre is, which is how the sheet's own width
/// in millimetres becomes a number to fit against.
const PX_PER_MM: f64 = 96.0 / 25.4;

/// The page a report is read on.
///
/// ```ignore
/// <ReportViewer definition=customer_statement() controls=|| view! { <SpanPicker .. /> }>
///     <Report definition=customer_statement() data=statement />
/// </ReportViewer>
/// ```
#[component]
pub fn report_viewer<T>(
    definition: ReportDefinition<T>,
    /// A link back to where the report was opened from.
    #[prop(optional)]
    back: Option<(&'static str, String)>,
    /// The report's own parameters, as controls on the toolbar.
    #[prop(optional, into)]
    controls: Option<ViewFn>,
    /// What the report was run with - the customer, the span, the record.
    /// Handed to the server when an export is asked for, because the worker
    /// draws the report again rather than being sent what is on screen.
    ///
    /// A signal, because a picker on this very toolbar changes it: read when
    /// the format is chosen rather than when the frame was drawn, or an
    /// export would be of whatever the screen opened on.
    #[prop(optional, into)]
    parameters: Option<Signal<Parameters>>,
    children: ChildrenFn,
) -> impl IntoView
where
    T: Send + Sync + 'static,
{
    let sheet_mm = f64::from(definition.page.width_mm());
    // What the definition declares, which is what the menu is measured
    // against rather than what it draws.
    let declared = StoredValue::new(definition.formats.clone());
    let title = StoredValue::new(definition.title.clone());
    let print_rules = StoredValue::new(print_rules(&definition.page));
    let back = StoredValue::new(back);
    let controls = StoredValue::new(controls);
    let children = StoredValue::new(children);

    // The report's own gate. A page that only fetched through a refusing
    // server function would answer an address it should not have drawn at all.
    let permission = definition.permission();
    let viewer = Viewer::get();
    let may_read = move || viewer.get().is_some_and(|user| user.can(permission));

    // Zero-height and full-width, so it measures the surface without being
    // resized by what is drawn on it.
    let gauge: NodeRef<html::Div> = NodeRef::new();
    let menu: NodeRef<html::Details> = NodeRef::new();

    let fit = RwSignal::new(true);
    let scale = RwSignal::new(1.0_f64);
    let resized = RwSignal::new(0_u32);

    Effect::new(move |_| {
        let handle = window_event_listener(leptos::ev::resize, move |_| {
            resized.update(|count| *count = count.wrapping_add(1));
        });

        on_cleanup(move || handle.remove());
    });

    // Runs in the browser only, after the first paint, so both builds render
    // the sheet at its own size and the fit is applied afterwards.
    Effect::new(move |_| {
        resized.track();

        if !fit.get() {
            scale.set(1.0);
            return;
        }

        let Some(node) = gauge.get() else {
            return;
        };

        let available = f64::from(node.client_width());
        let printed = sheet_mm * PX_PER_MM;

        scale.set(if printed > 0.0 {
            (available / printed).min(1.0)
        } else {
            1.0
        });
    });

    // What the export is doing, in one line under the toolbar. `None` is the
    // usual state: a report nobody has asked to write out.
    let progress = RwSignal::new(None::<Progress>);

    let report_id = definition.id;
    let bounded = matches!(definition.extent(), Extent::Bounded(_));
    let parameters =
        parameters.unwrap_or_else(|| Signal::derive(|| Parameters::Object(Map::new())));

    let alerts = Alerts::get();

    let choose = move |format: ExportFormat| {
        if let Some(node) = menu.get() {
            node.set_open(false);
        }

        if !declared.with_value(|declared| declared.contains(&format)) {
            alerts.post(Alert::warning(l!("report.export.not_offered")).message_box());
            return;
        }

        progress.set(Some(Progress::Working));

        // Which path a report takes is its definition's declaration, never a
        // guess made here: bounded comes back with its bytes, and anything
        // that grows with its data becomes a row a worker picks up.
        let asked = parameters.get_untracked();

        if bounded {
            write_now_and_download(report_id, asked, format, progress);
        } else {
            raise_and_wait(report_id, asked, format, progress);
        }
    };

    let frame = move || {
        view! {
        <section class="space-y-3">
            {back
                .get_value()
                .map(|(href, label)| {
                    view! {
                        <A
                            href=href
                            attr:class="inline-flex items-center gap-1 text-xs text-content-subtle hover:text-content"
                        >
                            <Icon icon=Icon::ArrowLeft size=IconSize::Xs />
                            {label}
                        </A>
                    }
                })}

            // Flat, not a card: a bar the sheet slides under wants square
            // corners, and the rounded ones showed the surface through them.
            //
            // Stuck to the top of the shell's scrolling region rather than to
            // the window. `main` is the only thing that scrolls here, so
            // `sticky` holds the bar against it while the sheet moves under;
            // `fixed` would take the bar out of the layout and lay it over the
            // top bar, which is the chrome this viewer is a page inside of.
            <div class="sticky top-0 z-20 flex flex-wrap items-center justify-between gap-3 border-y border-edge bg-surface-raised px-3 py-2 shadow-sm">
                <div class="flex min-w-0 flex-wrap items-center gap-3">
                    <h1 class="truncate text-base font-semibold tracking-tight text-content">
                        {title.get_value()}
                    </h1>
                    {controls.get_value().map(|controls| controls.run())}
                </div>

                <div class="flex items-center gap-2">
                    <button
                        type="button"
                        class="rounded-control border border-edge px-2 py-1 text-xs text-content hover:bg-surface-hover"
                        aria-pressed=move || fit.get().to_string()
                        on:click=move |_| fit.update(|on| *on = !*on)
                    >
                        {l!("report.fit_width")}
                    </button>

                    <button
                        type="button"
                        class="rounded-control border border-edge px-2 py-1 text-xs text-content hover:bg-surface-hover"
                        on:click=move |_| print_page()
                    >
                        {l!("report.print")}
                    </button>

                    {declared
                        .with_value(|declared| !declared.is_empty())
                        .then(|| {
                            view! {
                                <div class="flex items-center">
                                    // The default, which is what an export
                                    // usually means. The menu beside it is for
                                    // the exception.
                                    <button
                                        type="button"
                                        class="flex items-center gap-1 rounded-l-control border border-edge px-2 py-1 text-xs text-content hover:bg-surface-hover"
                                        on:click=move |_| choose(ExportFormat::Pdf)
                                    >
                                        <Icon icon=Icon::Download size=IconSize::Xs />
                                        {l!("report.export")}
                                        " "
                                        {ExportFormat::Pdf.label()}
                                    </button>

                                    <details node_ref=menu class="relative -ml-px">
                                        <summary
                                            class="flex cursor-pointer list-none items-center rounded-r-control border border-edge px-1.5 py-1 text-xs text-content hover:bg-surface-hover"
                                            aria-label=l!("report.export.formats")
                                        >
                                            <Icon icon=Icon::ChevronDown size=IconSize::Xs />
                                        </summary>
                                        <ul class="absolute right-0 z-20 mt-1 min-w-32 rounded-pop border border-edge bg-surface-raised py-1 shadow-pop">
                                            {ExportFormat::ALL
                                                .iter()
                                                .copied()
                                                .map(|format| {
                                                    view! {
                                                        <li>
                                                            <button
                                                                type="button"
                                                                class="block w-full px-3 py-1.5 text-left text-xs text-content hover:bg-surface-hover"
                                                                on:click=move |_| choose(format)
                                                            >
                                                                {format.label()}
                                                            </button>
                                                        </li>
                                                    }
                                                })
                                                .collect_view()}
                                        </ul>
                                    </details>
                                </div>
                            }
                        })}
                </div>
            </div>

            {move || progress.get().map(|progress| view! { <ExportNotice progress=progress /> })}

            <div class="overflow-x-auto rounded-card border border-edge bg-surface-sunken p-3 sm:p-6">
                <div node_ref=gauge class="h-0"></div>
                <div style=move || format!(
                    "zoom:{}",
                    scale.get(),
                )>{children.with_value(|children| children())}</div>
            </div>

            // Last, so that the gap `space-y-3` puts above every child but the
            // first lands on something that is not drawn.
            <style inner_html=print_rules.get_value()></style>
        </section>
        }
    };

    // Inside a boundary, because the session is a resource: read outside one
    // the server draws the refusal and the browser hydrates the report, which
    // is a node count that does not match.
    view! {
        <Suspense fallback=|| ()>
            {move || {
                if may_read() {
                    frame().into_any()
                } else {
                    view! { <Refused /> }.into_any()
                }
            }}
        </Suspense>
    }
}

/// What a report shows somebody who may not read it.
#[component]
fn refused() -> impl IntoView {
    view! {
        <section class="space-y-3">
            <h1 class="text-base font-semibold tracking-tight text-content">
                {l!("report.refused")}
            </h1>
            <p class="text-sm text-content-subtle">{l!("report.refused.detail")}</p>
        </section>
    }
}

/// How far an export has got, as the viewer needs to say it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Progress {
    /// Asked for, and nothing has come back yet.
    Working,
    /// Written, and here is where it is.
    Ready { href: String },
    /// Not written, and here is what the row says about why.
    Failed { reason: String },
    /// Still running when the viewer stopped asking. Not a failure: the file
    /// is a stored file and it will be there.
    Waiting,
}

/// One line under the toolbar: what the export is doing.
#[component]
fn export_notice(progress: Progress) -> impl IntoView {
    let (tone, body) = match progress {
        Progress::Working => (
            "border-edge text-content-muted",
            view! { <span>{l!("report.export.running")}</span> }.into_any(),
        ),
        Progress::Ready { href } => (
            "border-brand text-content",
            view! {
                <span>
                    {l!("report.export.ready")}
                    " "
                    <a href=href class="font-medium text-brand underline" download>
                        {l!("report.export.download")}
                    </a>
                </span>
            }
            .into_any(),
        ),
        Progress::Failed { reason } => (
            "border-danger text-content",
            view! { <span>{l!("report.export.failed")} " " {reason}</span> }.into_any(),
        ),
        Progress::Waiting => (
            "border-edge text-content-muted",
            view! { <span>{l!("report.export.still_running")}</span> }.into_any(),
        ),
    };

    view! {
        <p class=format!("rounded-control border px-3 py-2 text-xs print:hidden {tone}")>{body}</p>
    }
}

/// How long the viewer keeps asking after a running export, and how often.
///
/// Bounded on purpose. A report that is still running after this has not
/// failed - it is a job, and the file will be there - so the viewer says so
/// and stops rather than asking for ever on a page somebody left open.
const ASK_EVERY: Duration = Duration::from_secs(2);
const GIVE_UP_AFTER: Duration = Duration::from_secs(120);

/// The bounded path: the bytes come back and the browser saves them.
fn write_now_and_download(
    report_id: &'static str,
    parameters: Parameters,
    format: ExportFormat,
    progress: RwSignal<Option<Progress>>,
) {
    leptos::task::spawn_local(async move {
        match write_now(report_id.to_owned(), parameters, format).await {
            Ok(written) => {
                export::download(&written.file_name, &written.contents);
                progress.set(None);
            }
            Err(err) => progress.set(Some(Progress::Failed {
                reason: err.to_string(),
            })),
        }
    });
}

/// The unbounded path: a row, then asking after it until it is done.
fn raise_and_wait(
    report_id: &'static str,
    parameters: Parameters,
    format: ExportFormat,
    progress: RwSignal<Option<Progress>>,
) {
    leptos::task::spawn_local(async move {
        match raise_export(report_id.to_owned(), parameters, format).await {
            Ok(request) => ask_after(request.id, progress, Duration::ZERO),
            Err(err) => progress.set(Some(Progress::Failed {
                reason: err.to_string(),
            })),
        }
    });
}

/// Ask after a running export, and keep asking until it is one thing or the
/// other.
///
/// Nothing here blocks the report: it stays readable, and a second format can
/// be asked for while this one runs. Closing the page cancels nothing - the
/// bytes become a stored file either way.
fn ask_after(id: Uuid, progress: RwSignal<Option<Progress>>, waited: Duration) {
    leptos::task::spawn_local(async move {
        match export_state(id).await {
            Ok(request) if request.state == ExportState::Ready => {
                let href = request
                    .file_id
                    .map(content::content_url)
                    .unwrap_or_default();

                progress.set(Some(Progress::Ready { href }));
            }
            Ok(request) if request.state == ExportState::Failed => {
                progress.set(Some(Progress::Failed {
                    reason: request.failure.unwrap_or_default(),
                }));
            }
            Ok(_) if waited >= GIVE_UP_AFTER => progress.set(Some(Progress::Waiting)),
            Ok(_) => {
                set_timeout(
                    move || ask_after(id, progress, waited + ASK_EVERY),
                    ASK_EVERY,
                );
            }
            Err(err) => progress.set(Some(Progress::Failed {
                reason: err.to_string(),
            })),
        }
    });
}

/// What printing keeps, and at what size.
///
/// The sheet is the page: everything that is not the sheet, does not contain
/// it and is not inside it stops being drawn, and what is left of the chain
/// down to it gives up its width, its scrolling and its fit-to-width zoom. The
/// paper is the one the definition names, so a landscape report prints
/// landscape without anybody choosing it in the browser's dialog.
fn print_rules(page: &PageSetup) -> String {
    format!(
        "@page{{size:{width}mm {height}mm;margin:0}}{PRINT_MEDIA}",
        width = page.width_mm(),
        height = page.height_mm(),
    )
}

/// The half of [`print_rules`] that does not depend on the paper.
const PRINT_MEDIA: &str = concat!(
    "@media print{",
    "body :not(:has([data-report-sheet])):not([data-report-sheet]):not([data-report-sheet] *)",
    "{display:none!important}",
    "body :has([data-report-sheet])",
    "{display:block!important;overflow:visible!important;zoom:1!important;",
    "width:auto!important;max-width:none!important;height:auto!important;",
    "max-height:none!important;margin:0!important;padding:0!important;",
    "border:0!important;border-radius:0!important;box-shadow:none!important;",
    "background:transparent!important}",
    "[data-report-sheet]{zoom:1!important;margin:0!important;box-shadow:none!important}",
    "}"
);

/// Ask the browser to print what is on the screen.
#[cfg(feature = "hydrate")]
fn print_page() {
    let _ = leptos::prelude::window().print();
}

/// Unreachable on the server: nothing renders a click there.
#[cfg(not(feature = "hydrate"))]
const fn print_page() {}
