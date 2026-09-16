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

use phonix_core::report::PageSetup;

use super::{ExportFormat, ReportDefinition};
use crate::icons::{Icon, IconSize};
use crate::l;

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
    /// What to do when a format is chosen. Choosing one raises an export; the
    /// bytes do not come back from the click.
    #[prop(optional)]
    on_export: Option<Callback<ExportFormat>>,
    children: Children,
) -> impl IntoView
where
    T: Send + Sync + 'static,
{
    let sheet_mm = f64::from(definition.page.width_mm());
    let formats = definition.formats.clone();
    let title = definition.title.clone();
    let print_rules = print_rules(&definition.page);

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

    let choose = move |format: ExportFormat| {
        if let Some(node) = menu.get() {
            node.set_open(false);
        }

        if let Some(on_export) = on_export {
            on_export.run(format);
        }
    };

    view! {
        <section class="space-y-3">
            {back
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
                        {title}
                    </h1>
                    {controls.map(|controls| controls.run())}
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

                    {(!formats.is_empty())
                        .then(|| {
                            view! {
                                <details node_ref=menu class="relative">
                                    <summary class="flex cursor-pointer list-none items-center gap-1 rounded-control border border-edge px-2 py-1 text-xs text-content hover:bg-surface-hover">
                                        <Icon icon=Icon::Download size=IconSize::Xs />
                                        {l!("report.export")}
                                        <Icon icon=Icon::ChevronDown size=IconSize::Xs />
                                    </summary>
                                    <ul class="absolute right-0 z-20 mt-1 min-w-32 rounded-pop border border-edge bg-surface-raised py-1 shadow-pop">
                                        {formats
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
                            }
                        })}
                </div>
            </div>

            <div class="overflow-x-auto rounded-card border border-edge bg-surface-sunken p-3 sm:p-6">
                <div node_ref=gauge class="h-0"></div>
                <div style=move || format!("zoom:{}", scale.get())>{children()}</div>
            </div>

            // Last, so that the gap `space-y-3` puts above every child but the
            // first lands on something that is not drawn.
            <style inner_html=print_rules></style>
        </section>
    }
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
