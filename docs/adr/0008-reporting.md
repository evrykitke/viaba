# ADR 0008 — The reporting engine

Status: accepted; built as far as the exports. The band model, the looks, the
definition, the renderer, the viewer, three reports, the document settings, the
index, grouping, the paginator and the export path are in; charts, drill-down,
the spreadsheet and the four statements are queued in `BACKLOG.md`.
Date: 2026-09-16
Amended: 2026-09-16 — section 9. The PDF is printed by a browser rather than
written by a band writer.
Amended: 2026-09-17 — section 6.1. The engine draws the workspace's own words at
the head of every report; a definition does not declare them.

This workspace has four statements. The trial balance, the balance sheet, the
profit and loss and the customer statement are a thousand lines of hand-written
markup in [`pages/sales/reports/`](../../crates/phonix-web/src/pages/sales/reports/),
and each of them decides for itself what a heading is, what a total row looks
like and how a figure is aligned. They agree today because one person wrote all
four in a week. The fifth will not agree, and neither will the change somebody
makes to the third.

None of them prints. There is no page, so there is no page break, so there is
nothing to attach to an email or file — which is most of what a statement is
for. `OrganizationProfile::logo_file_id` is documented in
[`organization.rs`](../../crates/phonix-core/src/organization.rs) as "the
uploaded logo that goes on documents" and no document draws it. And a grid row
opens a form: there is no way to hand somebody the record behind a row.

This record specifies the engine that answers all of that at once — one way to
describe a report, one component that draws it, one frame it is read in, and
one set of writers that turn it into a file. The four statements move onto it
last, and when they have there is one way of drawing a report here and no
second path left to drift from it.

---

## 1. A definition is Rust, and a field names a real field

A report is a value of `ReportDefinition<T>`, built the way
[`ui::table::config`](../../crates/phonix-web/src/ui/table/config/) already
builds a grid: one file per report under `ui/report/config/`, a builder, and
closures over a typed row. `ui/report/` is a peer of `ui/table/` and reuses
`Cell` and `Align` rather than growing a second set of them.

The obvious alternative is a data file — TOML naming columns by string,
resolved against a row at runtime — and it is the one this record rules out
first, because every other decision below leans on it.

**A field that names something that does not exist must fail to compile.** That
is the property `Column::new` has today, and a report has strictly more ways to
lose it: a field, a group key, a total, a permission and an i18n key, each of
which a string file turns into a runtime lookup that succeeds until the day the
struct is renamed. Buying a data file means inventing a dynamic value layer
nothing else in this tree has, and paying for it in every one of those places.

What a data file would buy is a report somebody edits without a deploy — which
is a report designer, and §6 says why that is not being built.

### 1.1 What `T` is: one value, not a vector of rows

*Settled 2026-09-16 while the customer statement was built, and true of every
report since.*

`T` is **the report's own data** — the whole of it, as one value — and not the
type of a row. A customer statement is a `CustomerStatement`: a party, a span,
an opening balance, its lines and its ageing. The bands read that value
directly, and the detail band reaches the sequence inside it through
`Band::lines(read, fields)`, whose `fields` are closures over the *line* type
and are erased behind one reader built from the same vector the headings come
from.

The alternative — `ReportDefinition<Row>`, the report being a `Vec<Row>` — is
the shape a grid has, and it cannot say what a document says. A letterhead
showing a customer and a closing balance over lines that are transactions is
two types meeting in one definition, and a report that only knew its rows would
have to be handed its letterhead separately by every screen that drew one.
That is the drift this record exists to prevent, one level up.

Three consequences worth stating, because each is a thing somebody would
otherwise invent again:

* a band drawn once — the letterhead, the totals — reads `T`, so it needs no
  data of its own;
* the headings of a detail band and the cells under them are built from one
  vector of `Field<L>`, so they cannot fall out of step;
* a report whose data is a `Page<T>` — the product list — is a report over one
  value like any other, and how many rows that page holds is the read's
  business rather than the definition's.

Grouping sits on the same distinction and is worth spelling out, because it is
where getting it wrong would cost the most. A detail band's grouping reads `L`:
what two lines share, and what the group's subtotal adds up — so a subtotal is
always the arithmetic of the rows above it. A figure that *follows* a group and
is not that — a gross profit, which is one section taken from another — is read
from `T` instead, through `Band::result_after`, and is drawn unlike a subtotal.
Which of the two a number comes from is what makes every subtotal on the page
trustworthy, and it is a property of the types rather than of anybody's care.

## 2. The band model is in `phonix-core`, and it is a new top-level module

`phonix_core::report` holds the band kinds — report header, page header, group
header, detail, group footer, report footer, page footer — with
`ReportKind::{List, Document}`, `PageSetup`, `LogoPlacement`, alignment, the
resolved metrics of §5, the chart geometry of §7 and the paginator of §8.

This is a new top-level module in the crate that is meant not to grow one, so
the argument had better be in a record rather than in a commit message.

**Two consumers need the same numbers on the same day.** The screen draws a
report and a writer turns the same report into a file, and they are in
different crates: `phonix-web` renders it and `phonix-services` writes it. A
model that lived in either would be imported by the other across a boundary
that exists precisely to keep the browser's crate free of I/O, or copied — and
two definitions of where a margin is, is a document that stops matching the one
on screen, discovered by somebody holding it.

*Amended 2026-09-16: the PDF is no longer one of those consumers — it is the
screen's own page, printed. The band model is what the screen, the CSV and the
paginator share, and that is still three.*

It passes 0001's test for `core`: it is mechanism, not meaning. It knows a band
is drawn above the detail rows; it does not know what a statement is. The wasm
rule holds as written — plain data with serde, no leptos, no sqlx, and
`cargo check -p phonix-core --target wasm32-unknown-unknown` is what says so.

## 3. What a report may never do

**A definition reaches no datasource of its own and carries no SQL.** Its rows
arrive from a typed server function that already exists — the customer
statement from `app_books::report::CustomerStatement`, the product list from
the paged item read Inventory already has — and the definition describes what
to draw with them.

Every reference product in this category eventually grows a report that holds
its own query, and it is where they stop being safe: a query written into a
definition escapes the permission the read enforces, the tenancy the pool
resolves, and the paging that keeps a list from being read whole. This
workspace has four commits about unpaged reads already.

So a report is a *view over a read that somebody else is responsible for*. If a
report wants figures no read produces, the answer is a read in the app crate
that owns them, with the port, the permission and the paging that implies — not
a `SELECT` in `ui/report/config/`.

The same denial covers the clock.
[`pages/sales/reports/mod.rs`](../../crates/phonix-web/src/pages/sales/reports/mod.rs)
gives the reason and it is unchanged here: a span worked out during the render
and again during hydration is two different answers either side of midnight,
and a hydration mismatch takes every handler on the page down with it.

## 4. A report fills the content area, and the toolbar is part of the frame

The viewer is a page inside the application shell. The navigation and the
header stay exactly where they are; the report takes the whole of the content
area rather than sharing it with a panel. It is not a chrome-less takeover of
the window — somebody reading a statement is still somewhere in the
application, and still has to leave.

A report is read at the width it will print at. One shown in a column with
something beside it is read at neither that width nor the screen's.

**The toolbar is the frame's, not the report's** — the export menu, print, page
navigation, fit-to-width, and the report's own parameters. A customer picker
and a span picker are controls on it rather than a header above it, which is
what keeps the report itself the whole of the page. The formats are one
dropdown and not a row of buttons, because a row of buttons grows by one every
time a writer lands and the twentieth report would have grown its own.

## 5. A look is measurements, not a stylesheet

Three looks, named and chosen between. **Modern** is the default and matches
the rest of the application: generous spacing, hairline rules, colour for
emphasis. **Compact** is the RDLC look — dense rows, small type, full
gridlines, no colour — and is the one chosen when rows to a page is what
matters. **Professional** is for something a customer receives: a strong rule
under the letterhead, wider margins, restrained colour, totals given weight.

`ReportTheme` resolves to a metrics value in `phonix-core`: the type scale,
band heights, cell padding, rule weights, and where a rule is drawn at all. Not
to a class name.

**A PDF cannot read a stylesheet.** A look that existed as CSS would be a
screen that looks one way and a file that looks another — the failure §2 put
the band model in core to avoid. The paginator needs the same numbers for a
third reason: how many rows reach a page is a consequence of the look, and
"Compact fits more rows on a page than Modern" is then a unit test rather than
an intention.

The three looks are one set of bands measured three ways, not three sets of
markup. A fourth look is a commit.

## 6. A layout is code; a document setting is the tenant's

A tenant keeps document settings, per document type, in administration: the
look, the paper size and orientation, whether the logo is drawn and where and
at what height, and the free text in the header and the footer. An invoice that
cannot carry the tenant's own payment terms is one they will keep producing
outside the system.

That list is the whole of it, and the boundary is the point. **This is not a
report designer.** Nothing on that screen moves a band, adds a column or binds
a field. The layout is typed Rust and stays that way.

The split is the one [`config/numbering/`](../../config/numbering/) already
makes: the app declares the question — which documents this workspace issues,
and what they should look like out of the box, in `config/documents/<app>.toml`
validated at start-up — and the tenant owns the answer, in
`core.document_settings`. A document type declared with no numbering series is
a validation error, because the set of documents a workspace issues is already
written down once.

Two rules fall out of it. The look column holds one of the three names and
refuses anything else, since a free-text theme is the designer arriving through
the back door. And the header and footer text is the tenant's own words: it is
stored as words, it is never an i18n key, and it is never looked up in the
catalogue.

A definition names its own look and the settings override it. A list report has
no settings row at all — document settings are per document type, and a product
list is not a document.

Why not a designer: a layout the tenant owns is a layout nobody can change.
Once somebody has dragged a band, every fix to that report is a migration of
their drawing, and the compile-time binding §1 exists for is gone on the first
save. The reference products that offer one spend the rest of their lives
supporting it.

### 6.1 The letterhead is the engine's, not the definition's

*Amended 2026-09-17, after a printed statement was read by somebody who had to
ask who had sent it.*

Section 6 said a tenant keeps the mark and the free text, and left what a
document says about the workspace at that. It was not enough. A statement and a
receipt go to somebody outside the workspace, and they carried a logo, a title
and no way to tell who had issued them - no address, no registration number, no
tax identifier, nothing to reply to.

**The engine draws the workspace's own words at the head of every report**,
from `organization::Letterhead`, which now carries the address, the ways to
reach the workspace and what it is registered as. A definition does not declare
it, cannot move it and cannot leave it out, so the head of a trial balance says
the same thing in the same order as the head of a receipt, and a report added
next year says it without knowing it has to. What a report's own header band
says - the customer and the span on a statement, the payment on a receipt -
remains the definition's and remains where it was, under the letterhead.

The boundary §6 draws is untouched, and the distinction is the point. This is
not a band the tenant moved: nobody chooses it, nobody places it, and there is
nothing on the settings screen that changes it. It is drawn from what the
workspace has already told us about itself on the organization screen, which is
the same source the outbox relay and the mailer read. A field nobody filled in
is not a blank line - it is absent - so the block is as complete as the profile
is and never more.

What the tenant still keeps is what §6 already listed: the look, the paper,
whether the mark is drawn and where, and the free text at the head and the foot.
**This is still not a report designer.**

## 7. A chart is a band the server drew

Bars, columns including stacked, lines, areas, pie and donut — drawn as inline
SVG from the report's own rows and its own group subtotals, from geometry
computed in `phonix_core::report`. A report whose only band is a chart is a
report.

A chart library is the wrong answer twice. It draws nothing during the server's
render, so the first thing that happens in the browser is the hydration
mismatch that kills every handler on the page. And it draws nothing at all into
a PDF, so the file that gets sent would be the lesser copy of the report — the
opposite of the point, since the file is the one that leaves the building.

So: no `<canvas>`, no chart dependency, no clock. Axis, ticks, labels and
legend are computed from the numbers rather than by a library.

*Amended 2026-09-16: the second half of this — the PDF writer drawing from the
same geometry — is gone with the writer. The browser prints the SVG the screen
drew, which is the same picture by construction rather than by agreement.*

## 8. The page a report is printed on

*Amended 2026-09-16. This section argued for a PDF written here by a band
writer, and against printing from a browser. The second half of that is still
right and the first is not — see §9.1.*

The objection to a browser was that "a statement that can only exist while
somebody has it open cannot be attached, filed, or produced by anything that is
not a person at a keyboard". That is an objection to printing on the *client*,
and it stands. It says nothing about a browser on the server: a headless one,
opened by the job, printing a page nobody is looking at, storing the bytes as
an ordinary file. No person, no open tab, and the same artefact at the end.

What this section got right is that a report has to be read in pieces, and
what it got wrong is that one answer serves both places it is read.

**A printed page ends where the paper does**, and the browser works that out
from `@page` - the definition's own paper, which the viewer's print rules
already put there.

**A page on screen ends where reading stops being comfortable**, which is a row
count: ten, with Previous and Next on the toolbar. It lives in
`ui::report::paging` because it is a screen's question, and it needs no
millimetres to answer it.

The paginator that tried to answer both - bands measured against a sheet, in
`phonix_core::report::paginate` - is gone. It was written for the band writer
and deleted with it. The look still reaches `phonix-core` as metrics, because
the screen and the CSV writer read them, and "Compact fits more rows on a page
than Modern" is now something a stylesheet says rather than something a unit
test proves.

## 9. An export is a job when the work is unbounded, and there is one writer

A statement over a year of a busy ledger is minutes of rendering. A server
function returning those bytes holds a connection open for all of it and loses
the work the moment somebody closes the tab. So that export raises a row in
`core.report_exports`, a worker renders it, and the bytes are stored as an
ordinary file.

A receipt is one payment and its allocations. It renders in the request and
comes back at once, because a round trip through a queue to produce one page is
a slower answer and a second set of failures bought for nothing.

**The definition declares which, and says what bounds it.** Not a size
threshold, and not a switch on a screen: this is the judgement this codebase
already demands about an unpaged read — *a query left unpaged on purpose gets
one line on its doc comment saying what bounds it* — settled per report, in
code, by whoever knows what the report is over. Growing with the data means a
job. Bounded by the record it is about means inline.

That only stays honest if there is **one writer per format**. A writer is a
plain function from a definition, its rows and its settings to bytes; it lives
in `phonix-services`, it takes no pool and no worker context, and the request
path and the exporter call the same one. Two writers that drifted would be a
receipt and a statement that disagree about what a CSV is.

### 9.1 The PDF is the report's own page, printed

*Amended 2026-09-16, after the first exported statement was put beside the
screen.*

A band writer was built for the PDF and it is not the right answer. It shares
the report's *measurements* with the screen — band heights, the page, the
padding, the rules — and nothing else: no font metrics, no colour tokens, no
wrapping, no image decoder. So the first file anybody downloaded printed its
letterhead on top of itself, and the three things queued to close the gap — the
mark, the chart, and a font embedded so a Chinese report is not blank boxes —
were three implementations of what a browser already has.

**A PDF export opens the report's own address in a headless browser and takes
what it prints.** The engine that draws the screen draws the file, so the two
are one document rather than a resemblance, and they stay that way when
somebody changes the stylesheet. `@page` already carries the definition's own
paper, and the viewer's print rules already leave the sheet alone on the page.

What it costs, said plainly: a browser binary on every box that runs the
exporter, named in config and validated at boot like every other path, and a
process per export with a timeout on it. A build with no browser fails fast
rather than at the first export.

What it does not change: the CSV is still a writer in `phonix-services`, the
band model is still what the screen and that writer share, and
`phonix_core::report::paginate` stays — it is what the viewer's own page
navigation reads, and CSS decides where a *printed* page ends. A format that is
not a page — the spreadsheet — is still a writer and a line on an enum.

The exporter is a fourth loop beside the verifier, the relay and the sweeper in
[`jobs.rs`](../../crates/phonix-server/src/jobs.rs), and it has the shape
`files::verify` already has: claim a row, do the work, write the outcome and
its event in one transaction. Raising a request dispatches it immediately, the
way an upload is claimed the moment its bytes are down, and the loop is the
safety net for a process that died mid-job rather than the normal path. The
file's name is deterministic from the request id, so a retry cannot leave two.

Adding a format after the third should be a writer and a line on an enum. If it
is not, the design has gone wrong, and that is worth saying out loud rather
than working around.

## 10. Who may run a report

A definition carries the permission it needs, and the index lists only what the
viewer may run. `Pages.Accounting.Reports` already gates the four statements as
one permission; a report the engine serves is gated the same way rather than
being open because it is new. An address typed by hand is refused, not merely
unlisted.

**The worker re-checks before it renders.** A job has no caller of its own, so
the row records who asked — and a grant withdrawn between the request and the
run has to stop the render. An export that ran as nobody would be a way to read
a report through the queue that the screen refuses.

## 11. The order of work, and the two migrations

The record, the model, the looks, the definition, the renderer, the viewer;
then one report of each kind — the customer statement first, because if the
band model cannot draw what `customer_statement.rs` draws by hand then the
model is wrong, and finding that out before three more reports are built on it
is the cheap version. Then the logo, the receipt and the row action that opens
it, the document settings and the screen that keeps them; then the index,
grouping, charts, drill-down and the paginator; then the export row, the
exporter, CSV, PDF and XLSX; and the remaining three statements last, onto all
of it at once.

**Two migrations, and only two.** `core/0024` is the document settings table
and `core/0025` is the export request. An item that finds it needs a third
should say so rather than adding one quietly.

The queue stops at seven checkpoints. Most of this puts something on a screen,
and a `cargo check` says nothing about whether a screen is right.

## 12. What is deliberately not built

* **The report row action on every other grid.** It lands on the customer
  receipts list only. The action is general — it names a definition and a row's
  id — but an invoice, a delivery note, a purchase order and a credit note each
  need a document designed for them. Four items, not four lines.
* **A report designer.** §6. A tenant keeps document settings, not layouts.
* **DOCX and HTML export.** The reference products offer both. PDF, XLSX and
  CSV cover what a report is actually sent as, and nobody has asked for either.
* **A look somebody authors.** §5. Three, chosen from.
* **An exports history screen.** A finished export is an ordinary stored file,
  so nothing is lost; nothing lists what a person has run. Worth having once
  there is enough of it to list.
* **Emailing an export.** The bytes being a stored file makes this small, and
  sending a customer their statement is the obvious next thing. It is still a
  decision somebody makes rather than a gap.
* **Switching the look from the viewer's toolbar.** Useful for comparing the
  three, and refused because the look is a document setting: two places to
  change one thing is how they come to disagree.
