//! The documents settings tab: what each document this workspace issues looks
//! like.
//!
//! # It is a bounded set of answers
//!
//! Nothing on this screen moves a band, adds a column or binds a field. The
//! layout is code, and this is the paper it is printed on, the look it is
//! drawn in, whether the mark appears and the tenant's own words at the head
//! and foot. See `docs/adr/0008-reporting.md` §6 for why that line is where it
//! is; a field here that seems to need to cross it is a finding, not a field.
//!
//! # The preview is a sample
//!
//! Drawn against a made-up document rather than a real one, for the reason the
//! numbering tab previews a format against a sample counter. It also means the
//! three looks can be compared side by side without running a report.

use leptos::prelude::*;
use phonix_core::form::Submission;
use phonix_core::report::{
    Align, DocumentSettings, Logo, LogoPlacement, Orientation, PaperSize, ReportTheme,
};

use crate::components::page::{GhostButton, Panel, PrimaryButton};
use crate::icons::Icon;
use crate::l;
use crate::server_fns::admin_fns::save_document_settings;
use crate::ui::alert::{Alert, Alerts};
use crate::ui::card::CollapsibleCard;
use crate::ui::report::Report;
use crate::ui::report::config::sample::{Sample, sample};
use crate::ui::table::DataGrid;
use crate::ui::table::config::document_settings::document_settings_grid;

#[component]
pub fn documents_tab() -> impl IntoView {
    let editing: RwSignal<Option<DocumentSettings>> = RwSignal::new(None);

    // Bumped after a save, which rebuilds the grid and so re-fetches it - the
    // arrangement the numbering and currencies tabs use, and for the reason
    // given there.
    let version = RwSignal::new(0_u32);

    let build = move || {
        document_settings_grid(Callback::new(move |row: DocumentSettings| {
            editing.set(Some(row));
        }))
    };

    view! {
        <div class="space-y-3">
            // Open, because this card is the tab - see the same note on the
            // organization profile.
            <CollapsibleCard
                title=l!("documents.title")
                detail=l!("documents.description")
                icon=Icon::FileText
                open=true
            >
                {move || {
                    version.track();
                    view! { <DataGrid config=build() /> }
                }}
            </CollapsibleCard>

            {move || {
                editing
                    .get()
                    .map(|settings| {
                        view! {
                            <DocumentEditor
                                settings=settings
                                saved=move || version.update(|count| *count = count.wrapping_add(1))
                                close=move || editing.set(None)
                            />
                        }
                    })
            }}
        </div>
    }
}

/// One document type: how it is drawn, and what it says at the head and foot.
#[component]
fn document_editor(
    settings: DocumentSettings,
    saved: impl Fn() + Copy + Send + Sync + 'static,
    close: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let alerts = Alerts::get();
    let title = settings.document_type.replace('_', " ");
    // Stored rather than cloned into the closure, so `chosen` stays `Copy`
    // and both the form and the preview can read it.
    let document_type = StoredValue::new(settings.document_type.clone());

    let theme = RwSignal::new(settings.theme);
    let paper = RwSignal::new(settings.paper);
    let orientation = RwSignal::new(settings.orientation);
    let logo = RwSignal::new(settings.logo);
    let header_text = RwSignal::new(settings.header_text.unwrap_or_default());
    let footer_text = RwSignal::new(settings.footer_text.unwrap_or_default());
    let pending = RwSignal::new(false);

    // What the form is about to send, and what the sample is drawn from. One
    // value rather than two, so the preview cannot show something the save
    // would not store.
    let chosen = move || DocumentSettings {
        document_type: document_type.get_value(),
        theme: theme.get(),
        paper: paper.get(),
        orientation: orientation.get(),
        logo: logo.get(),
        header_text: some_words(&header_text.get()),
        footer_text: some_words(&footer_text.get()),
    };

    let save = move |()| {
        pending.set(true);
        let submission = chosen();

        leptos::task::spawn_local(async move {
            let result = save_document_settings(submission).await;
            pending.set(false);

            match result {
                Ok(Submission::Saved(())) => {
                    alerts.post(Alert::success(l!("documents.saved")));
                    saved();
                    close();
                }
                Ok(Submission::Rejected(errors)) => {
                    for error in errors {
                        alerts.post(Alert::failure(crate::i18n::t(&error.message)));
                    }
                }
                Err(err) => alerts.post(Alert::failure(err.to_string())),
            }
        });
    };

    view! {
        <Panel title=title>
            <div class="grid gap-4 lg:grid-cols-2">
                <div class="space-y-4">
                    <fieldset class="space-y-2">
                        <legend class="text-xs font-medium text-content-muted">
                            {l!("documents.look")}
                        </legend>
                        {ReportTheme::ALL
                            .iter()
                            .copied()
                            .map(|option| view! { <LookOption option=option chosen=theme /> })
                            .collect_view()}
                    </fieldset>

                    <div class="grid gap-3 sm:grid-cols-2">
                        <label class="block space-y-1">
                            <span class="text-xs font-medium text-content-muted">
                                {l!("documents.paper")}
                            </span>
                            <select
                                class="w-full"
                                on:change=move |ev| {
                                    if let Some(size) = PaperSize::parse(&event_target_value(&ev)) {
                                        paper.set(size);
                                    }
                                }
                            >
                                {PaperSize::ALL
                                    .iter()
                                    .copied()
                                    .map(|size| {
                                        view! {
                                            <option
                                                value=size.as_str()
                                                selected=move || paper.get() == size
                                            >
                                                {crate::i18n::t(&size.label())}
                                            </option>
                                        }
                                    })
                                    .collect_view()}
                            </select>
                        </label>

                        <label class="block space-y-1">
                            <span class="text-xs font-medium text-content-muted">
                                {l!("documents.orientation")}
                            </span>
                            <select
                                class="w-full"
                                on:change=move |ev| {
                                    if let Some(way) = Orientation::parse(&event_target_value(&ev)) {
                                        orientation.set(way);
                                    }
                                }
                            >
                                {Orientation::ALL
                                    .iter()
                                    .copied()
                                    .map(|way| {
                                        view! {
                                            <option
                                                value=way.as_str()
                                                selected=move || orientation.get() == way
                                            >
                                                {crate::i18n::t(&way.label())}
                                            </option>
                                        }
                                    })
                                    .collect_view()}
                            </select>
                        </label>
                    </div>

                    <LogoChoice logo=logo />

                    <Words label=l!("documents.header_text") value=header_text />
                    <Words label=l!("documents.footer_text") value=footer_text />
                    <p class="text-2xs text-content-subtle">{l!("documents.text_help")}</p>

                    <div class="flex flex-wrap gap-2">
                        <PrimaryButton
                            label=l!("common.save")
                            pending=pending
                            on_click=Callback::new(save)
                        />
                        <GhostButton
                            label=l!("common.cancel")
                            on_click=Callback::new(move |()| close())
                        />
                    </div>
                </div>

                <div class="space-y-1">
                    <p class="text-xs font-medium text-content-muted">{l!("documents.preview")}</p>
                    <p class="text-2xs text-content-subtle">{l!("documents.preview.help")}</p>

                    // Scaled down rather than fitted: this is a thumbnail of a
                    // page beside a form, not a report being read.
                    <div class="overflow-hidden rounded-card border border-edge bg-surface-sunken p-3">
                        <div style="zoom:0.45">
                            {move || {
                                let settings = chosen();
                                view! {
                                    <Report
                                        definition=sample(&settings)
                                        data=Sample::default()
                                    />
                                }
                            }}
                        </div>
                    </div>
                </div>
            </div>
        </Panel>
    }
}

/// One look, with the sentence that says what it is for.
#[component]
fn look_option(option: ReportTheme, chosen: RwSignal<ReportTheme>) -> impl IntoView {
    view! {
        <button
            type="button"
            class=move || {
                let state = if chosen.get() == option {
                    "border-brand bg-brand-subtle"
                } else {
                    "border-edge hover:bg-surface-hover"
                };
                format!("flex w-full flex-col gap-0.5 rounded-control border px-3 py-2 text-left {state}")
            }
            aria-pressed=move || if chosen.get() == option { "true" } else { "false" }
            on:click=move |_| chosen.set(option)
        >
            <span class="text-sm font-medium text-content">
                {crate::i18n::t(&option.label())}
            </span>
            <span class="text-2xs text-content-subtle">{crate::i18n::t(&option.help())}</span>
        </button>
    }
}

/// Whether the mark is drawn, where, and how tall.
#[component]
fn logo_choice(logo: RwSignal<Option<Logo>>) -> impl IntoView {
    let band = move || {
        logo.get()
            .map(|logo| logo.placement.band_str())
            .unwrap_or("")
    };
    let align = move || {
        logo.get()
            .map_or(Align::Start, |logo| logo.placement.align())
    };

    view! {
        <div class="grid gap-3 sm:grid-cols-3">
            <label class="block space-y-1 sm:col-span-2">
                <span class="text-xs font-medium text-content-muted">{l!("documents.logo")}</span>
                <select
                    class="w-full"
                    on:change=move |ev| {
                        let picked = event_target_value(&ev);
                        let height = logo
                            .get_untracked()
                            .map_or(Logo::DEFAULT_HEIGHT_MM, |logo| logo.height_mm);

                        logo.set(
                            LogoPlacement::parse(&picked, align())
                                .map(|placement| Logo::new(placement).at_height(height)),
                        );
                    }
                >
                    <option value="" selected=move || logo.get().is_none()>
                        {l!("documents.logo.none")}
                    </option>
                    <option
                        value="report_header"
                        selected=move || band() == "report_header"
                    >
                        {l!("documents.logo.report_header")}
                    </option>
                    <option value="page_header" selected=move || band() == "page_header">
                        {l!("documents.logo.page_header")}
                    </option>
                </select>
            </label>

            <label class="block space-y-1">
                <span class="text-xs font-medium text-content-muted">
                    {l!("documents.logo.height")}
                </span>
                <input
                    type="number"
                    class="w-full"
                    min="1"
                    max="100"
                    step="1"
                    disabled=move || logo.get().is_none()
                    prop:value=move || {
                        logo.get().map_or(Logo::DEFAULT_HEIGHT_MM, |logo| logo.height_mm)
                    }
                    on:input=move |ev| {
                        if let Ok(height) = event_target_value(&ev).parse::<f32>()
                            && let Some(current) = logo.get_untracked()
                        {
                            logo.set(Some(current.at_height(height)));
                        }
                    }
                />
            </label>
        </div>
    }
}

/// One of the two blocks of the tenant's own words.
#[component]
fn words(#[prop(into)] label: String, value: RwSignal<String>) -> impl IntoView {
    view! {
        <label class="block space-y-1">
            <span class="text-xs font-medium text-content-muted">{label}</span>
            <textarea
                class="w-full"
                rows="2"
                maxlength="500"
                prop:value=move || value.get()
                on:input=move |ev| value.set(event_target_value(&ev))
            ></textarea>
        </label>
    }
}

/// Blank is `None`: an empty box is no footer, not a footer of nothing.
fn some_words(value: &str) -> Option<String> {
    let trimmed = value.trim();

    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}
