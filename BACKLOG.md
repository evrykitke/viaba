# Backlog

The queue `/advance` works through. One item per iteration, top of `## Next`
first. Everything here is a *claim about what should be true when the item is
done*, not a task description — the difference matters, because the loop uses
it as the acceptance test.

## Format

```
- [ ] `crate-name` Subject line, as a noun phrase
      why: the reason this is worth doing, in one or two sentences
      touch: crates/.../file.rs, crates/.../other.rs      (optional hint)
      done: what is true afterwards that is not true now  (the acceptance test)
      verify: what to open in the running application, and what should be true
              on the screen                               (optional)
      stop: why the loop ends here rather than going on   (optional)
```

Only `why:` is required. `touch:` is a hint, not a fence — the loop may find
the work lives elsewhere and will say so. An item with a `blocked:` line is
skipped until that line is removed.

`verify:` says a build does not finish this item. A `cargo check` proves an
item compiles, which says nothing about whether a screen is right, so an item
carrying this line is committed and then moved to `## Awaiting verification`
rather than `## Done`. The user launches the application, looks, and moves it
on — or sends it back as a new item saying what was wrong.

`stop:` marks a checkpoint. The loop ends there rather than building further on
something nobody has seen. It is the loop's own brake, not a `blocked:` line:
the item is finished, the tree compiles, and the next item can be taken as soon
as the user has looked.

Keep items small enough that one of them is one commit. If an item needs three
commits it is three items.

---

## Next

> **The reporting engine**, queued 2026-09-16. Twenty-eight items in order: the
> decision record, the model, the renderer, the viewer that holds it, one
> report of each kind to prove it, the receipt a grid row opens, the three
> looks and the document settings an administrator keeps, then grouping, charts, drill-down,
> pagination, PDF, the spreadsheet exports, and the four existing statements
> re-cut onto it last.
>
> **A `cargo check` does not finish an item here.** Most of these put
> something on a screen, and the build says nothing about whether it is right.
> An item carrying a `verify:` line goes to `## Awaiting verification` when it
> is committed, not to `## Done`, and the user moves it on after looking at
> it in the running application. An item carrying `stop:` is a checkpoint: the
> loop ends there rather than building further on something unseen.
>
> **Eleven decisions the user made on 2026-09-16.** They are settled. An item
> below that looks like it reopens one has been misread.
>
> 1. **A definition is typed Rust, not a data file**, and it lives in
>    `crates/phonix-web/src/ui/report/config/`, one file per report, mirroring
>    `ui/table/config/` exactly. A field bound to something that does not
>    exist must fail to compile, and that is precisely what a TOML file cannot
>    do. The alternative - a file naming columns by string, resolved against a
>    row at runtime - means inventing a dynamic value layer nothing else in
>    this tree has, and losing the compile-time check on every field and every
>    i18n key to buy it.
> 2. **A report fills the page, inside the application shell.** The navigation
>    and the header stay exactly where they are; the report takes the whole of
>    the content area rather than sitting in a panel with other things beside
>    it. It is not a chrome-less takeover of the window.
> 3. **The viewer has a toolbar**, and the export formats are one dropdown on
>    it rather than a row of buttons that grows every time a writer lands.
> 4. **Charts are part of a report, not a decoration.** Bars, lines and the
>    rest are a band like any other, and a report may be nothing but a chart.
> 5. **PDF is written on the server**, from a paginator that is pure and
>    tested. A statement that can only be printed out of a browser cannot be
>    attached to an email or filed, which is most of what a statement is for.
> 6. **All four statements move onto the engine**, and **the band model lives
>    in `phonix-core`** because the screen and the PDF writer are both
>    consumers of it from the first day.
> 7. **A grid row can open its record as a report.** Most grids offer only
>    "open"; a document behind a row is a report, and the customer receipts
>    list is where that starts - `ui/table/config/payments.rs`, the money a
>    customer paid, not `receipts.rs`, which is what arrived from a supplier.
>    The user chose between the two on 2026-09-16.
> 8. **A tenant keeps document settings in administration** - per document
>    type: the paper, whether the logo is drawn and where, and the free text
>    in the header and footer. **This is not a report designer.** The layout
>    is typed Rust and stays that way; what a tenant owns is a bounded set of
>    answers, exactly the split `config/numbering` already makes between the
>    question an app asks and the answer a tenant gives. Anything that would
>    let somebody move a band, add a column or bind a field is out of scope
>    and stays out.
> 9. **Three looks, chosen and not authored** - Modern, Compact and
>    Professional. Compact follows the RDLC style: dense rows, gridlines,
>    small type, as many rows to a page as will go. A look resolves to
>    measurements in `phonix-core`, never to a stylesheet, because the PDF
>    writer and the paginator both need the numbers. A definition names its
>    look and the document settings override it.
> 10. **An export is a job when the work is unbounded.** A statement over a
>     year of a busy ledger is minutes of rendering, and a request that held a
>     connection open for all of it would lose the work the moment somebody
>     closed the tab: that raises a row, a worker renders it, and the bytes
>     are stored as an ordinary file. A receipt is one record and a known
>     number of lines; it renders in the request and comes back at once,
>     because a round trip through a queue to produce one page is a slower
>     answer and a second set of failures for no gain.
>
>     **The definition declares which, and says what bounds it.** This is the
>     same judgement this codebase already demands about an unpaged read -
>     *"a query left unpaged on purpose gets one line on its doc comment
>     saying what bounds it"* - and it is settled per report, in code, rather
>     than by a size threshold nobody can predict or a switch on a screen.
>     Growing with the data means a job. Bounded by the record it is about
>     means inline.
>
>     **This only stays honest if there is one writer.** A format writer is a
>     function from a definition, its rows and its settings to bytes. It lives
>     in `phonix-services`, it takes no pool and no worker context, and the
>     request path and the exporter both call the same one. Two writers that
>     drifted would be a receipt and a statement that disagree about what a
>     PDF looks like. The job machinery is the shape `files::verify` already
>     has - claim, work, write the outcome and its event in one transaction -
>     and the exporter is a fourth loop beside the verifier, the relay and
>     the sweeper.
> 11. **The user verifies in the running application.** The loop stops at the
>     seven checkpoints marked `stop:` below, plus the one at the end.
>
> **Nothing in this branch has run against a database.** Migrations
> `hr/0003` - `hr/0007` and `core/0023` are unapplied and compiler-checked
> only. The first checkpoint is where that gets found out, because it is the
> first time anything here is launched.
> **Two items below need a migration** - the document settings table,
> `core/0024`, and the export request table, `core/0025` - and they are the
> only two. An item that finds it needs another should say so rather than
> quietly adding one.
>
> **Deliberately not queued**, so that nothing below quietly grows into it:
>
> - **The report row action on every other grid.** It lands on the payments
>   list only. The action is general and a grid adopts it by naming a
>   definition, but the invoice, the delivery note, the purchase order and the
>   credit note each need a document designed for them - four more items, not
>   four more lines. Queue them when somebody has decided each should exist.
> - **A report designer.** See decision 8. A tenant keeps document settings,
>   not layouts.
> - **DOCX and HTML export.** The reference products offer both. Nobody has
>   asked for either, and PDF, XLSX and CSV cover what a report is actually
>   sent as.
> - **A look somebody authors.** Three named looks, chosen from. A fourth is a
>   commit, not a settings field, and a tenant-authored one is the report
>   designer that decision 8 rules out.
> - **An exports history screen.** A finished export is an ordinary stored
>   file, so it is not lost, but nothing lists what a person has run. Worth
>   having once there is enough of it to list; not before.
> - **Emailing an export.** The bytes being a stored file makes this small, and
>   sending a customer their statement is the obvious next thing. It is still a
>   decision somebody has to make rather than a gap.
> - **Switching the look from the viewer's toolbar.** It would be useful for
>   comparing the three, but the look is a document setting and two places to
>   change one thing is how they come to disagree. Ask if it is wanted.

> **Export moved to the front**, 2026-09-16, because the Export menu being
> absent was reported twice from the running application. The three items that
> deliver a working button - the request row, the exporter with CSV, and the
> viewer that waits for one - now come before the index, grouping and the
> charts. PDF and the spreadsheet stay where they were: they are more writers
> on a path that by then exists.

> **The exporter was one item and is three**, split 2026-09-16 while taking it.
> A writer, a stored file and a worker loop are three commits by the backlog's
> own rule, and the reason they read as one is that the original item was
> written before anybody knew where a definition could be seen from. The answer
> found while splitting: `phonix-server` depends on `phonix-web`, so the worker
> **can** reach the definitions - what crosses to `phonix-services`, which
> cannot, is a rendered report with the types erased. That is what makes one
> writer serve both paths, which is ADR 0008 section 9's whole requirement.

> **The export menu, as the user found it**, 2026-09-16, after the fifth
> checkpoint ran. Two things came back from looking at it. PDF is what most
> people mean when they say export, and it costs a menu and a second click on
> every report. And a menu drawn from what a definition declares is a menu of
> one item today, which reads as a broken control rather than as a deliberate
> one - there is no way to tell a format this report will never have from one
> whose writer has not landed, because neither is on it. Both are the
> toolbar's business. **Which formats can actually be written does not change
> here**, and the PDF writer stays where it is in this queue: until it lands
> the new default answers with the same sentence the menu gives.

> **PDF moved to the front**, 2026-09-16, on the user's instruction: the export
> button's default is PDF and nothing writes one, so the default answers every
> report with a refusal. The paginator comes with it and stays in front of it -
> a PDF writer that broke its own pages would be the second answer to a
> question `phonix-core` is about to answer for the screen as well, and this
> backlog has four commits undoing that shape. The charts, the collapsing
> group and the spreadsheet wait behind both.

> **The first PDF anybody downloaded was wrong**, 2026-09-16. Three faults in
> one file, all in front of the item that claimed the file and the screen were
> one document: the letterhead's four values printed on top of each other, the
> money columns came out left-aligned, and nothing stops a long cell running
> across the column beside it. The first two are drawing; the third is
> measurement, and it is why this sits in front of the mark and the words.

> **The PDF is printed by a browser**, decided 2026-09-16 after the first
> exported statement was put beside the screen. The hand-written band writer
> gets a report close and then drifts: it has no font metrics, no colour
> tokens, no wrapping, no image decoder, and every future change to the look
> would have to be made twice. Chrome is the engine that draws the screen, so
> printing the report's own page is exact by construction and stays exact.
> Three queued items die of it - the mark in the PDF, the chart in the PDF, and
> the font embedding the base-14 refusal implied - and `report/pdf.rs` is
> retired rather than finished. **The paginator stays**: it is what the
> viewer's own page navigation reads, and CSS decides where a printed page
> ends.
>
> **This run does not halt at a checkpoint.** The user is away and has said so:
> an item carrying `stop:` is committed, moved to `## Awaiting verification`
> like any other, and the loop takes the next one. The section is what they
> read when they come back.

- [ ] `phonix-web` A total between two groups
      why: a profit and loss reads gross profit straight after cost of sales
           and operating profit straight after the expenses, and the engine can
           only put a figure at the foot of a group or the foot of the report.
           The three results are all on the statement and all correct; they are
           read together at the bottom rather than where an accountant looks
           for them.
      touch: crates/phonix-web/src/ui/report/definition.rs,
             crates/phonix-web/src/ui/report/render.rs
      done: a definition can put a labelled figure after a named group, read
            from the report's own data rather than from the group's rows -
            because these three are differences between sections and not sums
            of one. The profit and loss draws its three where they belong, and
            the report footer keeps only what is genuinely the report's.
      verify: the profit and loss beside the one in the history: gross profit
              under cost of sales, operating profit under the expenses.

- [ ] `phonix-server` The dispatch that jobs.rs names and nothing answers to
      why: the module doc says "an upload is dispatched the moment its bytes
           are down - see `files::dispatch`", and there is no `files::dispatch`.
           The function is `files::upload::claim_for_verification`. ADR 0008 §9
           puts the exporter on that same shape, so the next person reading for
           it goes looking for a name that was never there.
      touch: crates/phonix-server/src/jobs.rs
      done: the comment names the function that exists.

- [ ] `docs` What ADR 0008 now owes the band model
      why: the record says a definition is typed Rust and says nothing about
           what it is typed *over*. The statement turned that into a decision:
           a report is drawn from one value, and the detail band reads a
           sequence inside it through `Band::lines` rather than the report
           being a vector of rows. Every item after it is that shape, and a
           reader of 0008 would not know.
      touch: docs/adr/0008-reporting.md
      done: section 1 says what a definition is over, and why a document's
            letterhead and its lines are two different types meeting in one
            definition.

- [ ] `phonix-web` The four dialogs that predate the modal
      why: `ui/table/toolbar` (the column menu), `ui/alert/host`, `ui/lookup`
           and `pages/admin/apps` each hand-rolled a dialog before `ui::modal`
           existed - at three different widths, some closing on Escape and
           some on a click outside, none of them moving focus. The commit that
           added the modal said converting them was its own item and then did
           not write it down, which is how four copies become five.
      touch: crates/phonix-web/src/ui/table/toolbar.rs,
             crates/phonix-web/src/ui/alert/host.rs,
             crates/phonix-web/src/ui/lookup/mod.rs,
             crates/phonix-web/src/pages/admin/apps.rs
      done: each of the four opens a `<Modal>` instead of its own markup, or
            carries one line saying why it cannot - the alert host in
            particular may be the one that genuinely differs, since it stacks
            and is not opened by anybody. Escape, the backdrop and focus then
            behave the same way everywhere, which is the whole point of having
            one.
      verify: the column menu on any grid, a lookup, and the app install
              dialog in Administration. All three should close on Escape and
              on the backdrop, and none on a click inside.

- [ ] `phonix-web` Three more editors that grow the page
      why: the same shape the settings tabs had, found while converting them:
           `master/party` edits an address below its panel, `master/tax` a
           rate, and `people/attendance` a day. Each pushes what you clicked
           off the screen on a list of any length. Not reported, and the same
           complaint applies.
      touch: crates/phonix-web/src/pages/master/party.rs,
             crates/phonix-web/src/pages/master/tax.rs,
             crates/phonix-web/src/pages/people/attendance.rs
      done: each opens its editor in a `<Modal>` and drops the `Panel` it was
            wrapped in. Attendance keeps whatever its `Show` is doing about
            the day being edited; this is about where the form appears, not
            when.
      verify: a party's addresses, a tax's rates, and an attendance day. Each
              editor should open over the list rather than under it.

## Awaiting verification

<!-- Committed, compiling, and not finished: each of these changed something
     the build cannot judge. The loop puts an item here when it carries a
     `verify:` line, with its commit sha and that line kept.

     The user launches the application, looks, and then either moves the item
     to `## Done` or writes a new item in `## Next` saying what was wrong. The
     loop never moves anything out of this section by itself. -->

- [x] `phonix-services` A letterhead that reads, in the PDF
      commit: "Four values on one line"
      why: the exported statement prints its party, its span, its currency note
           and its opening balance on one baseline in four equal columns, so
           all four overlap. The screen stacks a once-band by alignment - three
           groups, each a column of lines - and the writer draws it as a row.
           The detail band's alignment never reached the file at all: the call
           that carried it was written and then lost in a reformat, so a figure
           column is left-aligned in every PDF written so far.
      touch: crates/phonix-web/src/ui/report/definition.rs,
             crates/phonix-services/src/report/pdf.rs,
             crates/phonix-core/src/report/paginate.rs
      done: a once-band is drawn the way the screen draws it - the Start cells
            stacked at the left, the End cells stacked at the right, Center
            between them - and the detail band carries its alignment into the
            file, so a figure sits at the end of its column in both. A band
            that holds more lines than its declared height is measured at what
            it holds, by the paginator as well as by the writer, or the two
            disagree about where the page ends. A cell too wide for its column
            is cut with an ellipsis rather than drawn across its neighbour.
      verify: the statement, exported. The letterhead should read as it does on
              the screen, the amount and balance columns should line up at
              their right edge, and nothing should sit on top of anything.
      stop: the file is what somebody sends, and the only way to judge one is
            to open it.

- [x] `phonix-services` The report as a PDF a job wrote
      commit: "The file that leaves the building"
      why: the reason the paginator exists, and the format everything else was
           built towards. A statement that can only be printed out of a browser
           cannot be attached to an email or filed, and being sent to somebody
           is most of what a statement is for.
      touch: Cargo.toml, crates/phonix-services/, crates/phonix-server/src/jobs.rs
      done: a second writer on the exporter, drawing from the same definition
            the screen uses, in the same look, so the file and the screen are
            one document. The writer is a pure-Rust crate with no system
            dependency - confirm it builds on this toolchain before writing
            against it, and block the item saying so if it does not. The logo
            is read through `phonix_services::files::access`, not fetched over
            HTTP. There is no font in this repo, so text is the base-14
            encoding until one is embedded: a report in a locale that encoding
            cannot carry - `locales/zh.json` exists - fails the export with a
            reason rather than writing a file full of blank boxes. Failing
            loudly is the point: a job that wrote something unreadable is worse
            than one that refused. The three definitions declare
            `ExportFormat::Pdf` and `write_now` grows a PDF arm, because the
            toolbar's default button is PDF and until this lands it answers
            every report with "this document does not allow it".
      split: the mark and the tenant's own words at the head and foot of a
             document came out of this item on 2026-09-16 and are queued below.
             `Rendered` carries no logo, so putting one in the file is a change
             to what crosses the crate boundary as well as to the writer, and
             an image needs a decoder this workspace does not depend on yet.
      verify: export the statement as PDF and open the file. The letterhead,
              the look, the page breaks, the repeated header and the totals
              should match the screen - put the two side by side and switch the
              document's look to check they move together.
      stop: sixth checkpoint. This is the artefact that leaves the building,
            and the only way to judge a PDF is to open one.

- [x] `phonix-core` The paginator, which decides where a page ends
      commit: "Where a page ends"
      why: the PDF writer needs pages and a browser will not hand it any. This
           is the one piece of the engine whose correctness a test actually
           establishes - a group header orphaned at the foot of a page, a
           footer that does not fit, a detail band split in half - so it is
           pure, it is in core, and it is tested before anything draws with it.
      touch: crates/phonix-core/src/report/
      done: given a theme's metrics, a page setup and the rows, it returns
            pages with their bands placed, repeating the page header and any group
            header whose group continues onto the next page. A group header
            alone at the foot of a page moves to the next one. A chart band is
            placed whole or moved, never split. Tested: the orphan case, the
            exact-fit case, a single row taller than a page, and that Compact
            fits more rows on a page than Modern - the one assertion that
            proves the look actually reaches the arithmetic.
      split: the viewer's page navigation came out of this item on 2026-09-16
             and is queued below. The arithmetic is what the PDF writer needs
             and it is tested here; a toolbar control is a screen, and the two
             are two commits by this backlog's own rule.

- [x] `phonix-web` Grouping, and what a group adds up to
      commit: "What a group of rows comes to"
      why: a list report without groups is a grid with a letterhead. A group
           header, a group footer and a subtotal is what separates the two, and
           all three statements still to be migrated group.
      touch: crates/phonix-web/src/ui/report/, crates/phonix-core/src/report/
      done: a definition declares what it groups by and which fields total; the
            renderer draws a header and a footer per group; a subtotal is
            computed from the rows of that group rather than re-read. Money
            totals go through `Money` and never through `f64` - `Cell::number`
            is a display type, not an arithmetic one.
      verify: the product list grouped by its category. Add up one group's rows
              by hand and check the subtotal, then check the subtotals add to
              the report total.
      stop: third checkpoint. The list kind, the logo, the index and grouping
            are all in by here, and the three items after this build on the
            arithmetic.

- [x] `phonix-web` Where a report is found, and who may run it
      commit: "The list a report is found on, and the gate it carries"
      why: two reports exist and the only way to either is knowing its address.
           `Pages.Accounting.Reports` already gates the four statements as one
           permission; a report the engine serves must be gated the same way
           rather than being open because it is new.
      touch: crates/phonix-web/src/ui/report/, crates/phonix-web/src/navigation/
      done: an index lists every definition the viewer is permitted to run,
            grouped by the app that declares it, and a definition carries the
            permission it needs. A viewer without that permission is not shown
            the report and cannot reach it by typing the address.
      verify: the reports index, then sign in as an account without
              `Pages.Accounting.Reports` and confirm the statement is neither
              listed nor reachable by typing its address.

- [x] `phonix-web` The export button that does not need its menu
      commit: "The export that costs no menu"
      why: PDF is the answer often enough that it should not cost a menu, and
           the other two formats are the exception the menu is for.
      touch: crates/phonix-web/src/ui/report/viewer.rs
      done: the toolbar's export control is a split button - the button itself
            writes PDF, the chevron beside it opens the menu of every format.
            One path serves both, so a report that does not declare PDF answers
            the button with the message box the menu would have given, and the
            button is not drawn at all where the menu is not.
      verify: the items list, Report, then Export without opening anything: it
              answers that the document does not allow PDF. The chevron still
              opens the full list and CSV still writes as a job.
      stop: sixth checkpoint. Both items change the same control and nobody has
            seen either, and what the button does next is what the PDF writer
            decides.

- [x] `phonix-web` A format the document does not offer
      commit: "The menu that showed one of three"
      why: the export menu draws only the formats a definition declares, so it
           is a menu of one on all three reports that have one. Somebody
           looking for PDF is shown nothing at all rather than being told the
           document does not offer it.
      touch: crates/phonix-web/src/ui/report/viewer.rs,
             crates/phonix-core/i18n/en.json, locales/
      done: a report that offers any format offers all of `ExportFormat::ALL`,
            in that order. Choosing one its definition does not declare posts a
            message box saying the document does not allow that export option,
            and raises nothing - no row, no worker, no progress line under the
            toolbar. A declared format takes the path it takes now. A report
            that declares no format still draws no menu. The sentence is a key
            in all four catalogues.
      verify: a receipt from the payments list, Export. PDF and XLSX are both
              offered and each answers with the message box; CSV still comes
              straight back with the file.

- [x] `phonix-web` The viewer waits for its export
      commit: "The two paths behind one menu"
      The Export menu appears - the three reports that can be written out now
      register CSV, and the menu is drawn because a definition says so rather
      than because a screen remembered to. Choosing a format takes one of two
      paths, and **which one is the definition's declaration**: bounded writes
      in the request and the browser saves the bytes; anything that grows
      raises a row, says it is running, and the file arrives.

      The parameters are a **signal**, not a value. A picker on that very
      toolbar changes them, so they are read when the format is chosen rather
      than when the frame was drawn - otherwise an export would be of whatever
      the screen opened on. The worker draws the report again from those
      parameters instead of being sent what is on screen, which is what lets a
      closed tab and a reload both end at the same file.

      Immediate dispatch is wired, and this is the item that could do it: a
      server function cannot call into `phonix-server`, so `AppState` carries
      an unbounded channel and the exporter listens on it. **The row is
      written before the news is sent** - a worker told about a request the
      database had not accepted would render something nobody asked for - and
      losing the news costs a wait for the next poll, never the work. That
      mattered here: development polls every 120 seconds and the viewer stops
      asking at 120, so without the channel nearly every export would have
      ended on "still running".
      verify: **the fifth checkpoint, and none of it has run.** Start the
              application, then:

              1. The items list, Report, then Export - CSV. It grows with its
                 data, so it should say it is writing and then offer the file.
                 Reload while it runs: the file should still arrive, because
                 it is a job and not a page.
              2. A receipt from the payments list, Export - CSV. Bounded, so
                 it should come straight back with no waiting at all.
              3. A customer statement, Export - CSV, after picking a customer
                 and a span. The file should be that customer over that span,
                 not the one the screen opened on.

              There is also a request from the previous item waiting in
              `viaba_tenant_med_app_staging` - `product-list`, `7a6d665f` -
              which the loop should pick up within a poll of the first start.
      stop: fifth checkpoint. The queue, the worker and the storage run
            together here for the first time, and this is the only chance to
            see the two paths side by side before three more writers are built
            on them.

- [x] `phonix-server` The exporter, the fourth loop
      commit: "The fourth loop"
      The loop, beside the verifier, the relay and the sweeper: claim with
      `SKIP LOCKED`, render, write, store, mark - and a `running` row older
      than the claim timeout is claimed again, which is the whole recovery
      story for a process that died mid-job. Every failure lands on the row,
      because a worker that let an error out would leave a request saying
      `running` for ever and a screen waiting on it.

      The new part is `phonix_web::reports`: which reports the server can draw,
      the permission each needs, and a `render` from an id and its parameters
      to a `Rendered`. It is in `phonix-web` because a definition is closed
      over a row type only that crate knows, and `phonix-server` depends on it
      - so the worker asks there and what comes back has its types erased. The
      index of reports somebody may run will read the same list.

      **The worker renders as whoever asked**, rebuilt through
      `load_auth_user_by_id` so the permissions are the ones they hold now. An
      account deleted, or stripped of the report between the request and the
      run, stops it with a sentence on the row rather than rendering as
      nobody.

      Two things left honest rather than finished. Immediate dispatch has no
      caller: an upload is dispatched from the route that receives it, in this
      crate, and an export is raised by a server function in `phonix-web`,
      which cannot reach this one - closing that needs a channel on `AppState`
      and belongs with the screen that raises the request. Until then an export
      waits at most one poll interval. And a format with no writer fails by
      name rather than producing an empty file that looks like an answer.
      verify: **it has never run.** The dev server was stopped while this was
              written, so the path compiles and nothing has executed it. There
              is a real request waiting in `viaba_tenant_med_app_staging` -
              `product-list`, CSV, `7a6d665f` - which the exporter should pick
              up within a poll interval of the next start. Watch for
              `report exporter started` in the log, then
              `SELECT state, file_id, failure FROM core.report_exports`.

- [x] `phonix-web` The viewer's toolbar stays where it is
      commit: "A toolbar for more than the first screenful", then "Square
              corners on a bar the sheet slides under"
      `sticky top-0` against the shell's scrolling region, which is the only
      thing that scrolls here - `fixed` would have taken the bar out of the
      layout and laid it over the top bar it is supposed to sit inside. The
      surface's `overflow-x-auto` is a sibling rather than an ancestor, which
      is what lets sticky work at all; a scrolling ancestor would have pinned
      the bar to a box that is itself moving.
      verify: open a long report - the product list - and scroll. The toolbar
              should stay at the top of the content area with square corners
              and the sheet moving under it, and the navigation and top bar should not move at all.
              Then press Print and check the toolbar is still absent from the
              preview.

- [x] `phonix-web` A settings editor belongs in a modal, not under the grid
      commit: "A dialog, before there were five of them"
      `ui::modal`, a peer of `card` and `alert`: backdrop, Escape, a click
      outside but never one inside, focus in and back out again, `role=dialog`
      and `aria-modal`, in two widths. **Rendered is open** - there is no
      `open` prop, because every screen that wanted one already had
      `editing.get().map(...)`, and building the contents fresh is what
      re-seeds controls that read their opening value once.

      Documents, Currencies and Numbering open theirs in one now, and each
      dropped the `Panel` it was wrapped in - a card inside a dialog is a
      border inside a border. The four dialogs that predate this are left
      alone: converting them is its own item.

      One wrinkle worth knowing: the focus-restore is browser-only, so the
      server twin returns `Option<()>` rather than `()`. Both builds hand the
      same shape around, which is what stops the ssr build tripping a
      unit-binding lint on code that only does anything in a browser.
      verify: Administration. On Documents, Currencies and Numbering, click
              Edit on a row: the form should open over the grid, Escape and a
              click on the backdrop should close it, and a click inside should
              not. The page should not grow a second screenful.

- [x] `phonix-web` The product list, as the first list report
      commit: "A list, which is not a document"
      The list kind, proved: many rows under headings, a running page header
      so a torn-off sheet says what it is, and the count at the foot. Drawn in
      **Compact**, which is the look a list is for. It is not a document type,
      so no workspace setting reaches it - the paper a product list is printed
      on is not something a tenant has an opinion about, and that is the rule
      rather than an omission.

      It draws **a page of the read, not the read**: one bounded request
      through the `list_items` that already exists, a hundred rows, and the
      foot says how many there are altogether. Reading a list whole to print
      it is the hazard this backlog has four commits about.

      Reachable from the items list's toolbar, because a report nobody can
      find is the item after this one and this one should not wait for it.
      verify: the items list, the Report button. The columns should read as a
              list rather than a document, and the foot should say how many
              items there are altogether.

              **The page header does not repeat yet.** One sheet is drawn per
              run; repeating a header at a page boundary is arithmetic the
              paginator does, three items down, and doing it here would be
              that item written twice.

- [x] `phonix-core` A statement is a document nobody numbers
      commit: "A document that carries no number"
      The decision the item asked for: an unnumbered document lives in
      `config/documents/` **alone**, saying `numbered = false`, and not in
      `config/numbering/` as a series that issues nothing - a series handing
      out no numbers is a row whose only honest value is "not applicable".
      The cross-check still refuses a type with no series, which is what
      catches a typo in a `doc_type`; the flag is the deliberate line somebody
      writes to step around it, rather than a check that quietly stopped
      applying.

      `statement` is declared and the customer statement names it, so a
      workspace can give its statements their own paper, look and footer.
      verify: Administration, the Documents tab - Statement should now be in
              the list. Change its look and footer, then open
              `/accounting/reports/statement/<a customer>` and check it
              redrew. The receipt should be unaffected: they are separate
              documents with separate answers.

- [x] `phonix-web` Where the logo goes decides what sits beside it
      commit: "Three placements, three letterheads"
      The report header now arranges itself around its mark rather than
      stacking everything under it. **Left**: mark first, the header's own
      lines alongside. **Right**: the same line, mark last. **Centre**: the
      mark on a line of its own with everything under it, because a wide
      wordmark in the middle of a line has no room beside it. No mark draws
      exactly what it drew before, and the page-header placement is untouched
      - that one is a running header rather than a letterhead.
      verify: Administration, the Documents tab. Put the logo left, centre and
              right in turn on the `payment` document and watch the sample
              redraw each way, then open a receipt and check it matches.

- [x] `phonix-web` A report drawn to its document settings
      commit: "The setting that finally moves something"
      A document report resolves the workspace's answer for its own type and
      draws to it: the look, the paper and which way up, whether the mark is
      drawn and where, and the words at the head and the foot. The
      definition's choice is the default and the setting overrides it - for
      the mark too, so a settings row with no mark draws none rather than
      falling back to the definition's.

      Two structural things. The report body is now a closure, because the
      settings arrive after the first paint and a report drawn once would keep
      the look its definition named however the workspace had answered. And
      the shell's letterhead fetch became **one** question - `DocumentChrome`,
      the name and mark plus every document's settings - resolved once for a
      session, because the two halves are always wanted together and a second
      resource beside the first is how a shell grows six.

      A list report has no document type and is untouched, which is the rule:
      a product list is not something a tenant has an opinion about the paper
      of.
      verify: change the look, the footer text and the logo placement on the
              invoice in administration, then open a receipt and the customer
              statement. Both should redraw with the new settings, and each of
              the three looks should be recognisably itself - Compact fitting
              visibly more rows on a page than Modern. Turn the logo off and
              check the letterhead closes up rather than leaving a hole.

              **Read the note above about the statement first**: it is not a
              declared document type, so it keeps its definition's look and
              will not follow the invoice's settings. The receipt is the one
              that moves - change `payment`, not `sales_invoice`.

- [x] `phonix-web` The document settings tab, in administration
      commit: "The choices around a layout nobody can move"
      A Documents tab beside Numbering, built the same way: the grid is the
      tab, a row opens a panel below it, and a save bumps the version that
      rebuilds the grid. The look is three buttons carrying the sentence that
      says what each is *for* - `ReportTheme::label` and `help` now live in
      core beside the metrics, so the words reach a settings screen and a
      report index alike.

      The preview is a made-up document drawn by the real renderer, and it is
      built from the **same value the save sends**, so the sample cannot show
      something the save would not store. Blank text boxes are `None` rather
      than empty strings: an empty box is no footer, not a footer of nothing.
      No field on it moves a band.
      verify: Administration, the Documents tab. Change the look, the paper,
              the logo placement and the footer text on the invoice, save, and
              reopen - the values should still be there. Nothing is expected to
              redraw yet; the item below is what reads them.

- [x] `phonix-web` The receipt, and the grid row that opens it
      commit: "The document behind a row"
      A receipt over one payment, read through the existing `payment_detail`,
      with the allocations as its lines and each invoice number linking to the
      invoice. It declares itself **bounded** - one payment and its
      allocations, which is a page - so its export will render in the request
      rather than through the queue. `RowAction::report` is the general form:
      a link with the icon fixed, whose address is the caller's, so a grid
      adopts it by naming where its own record is drawn. The payments grid
      offers it through `when` on posted rows only, because a draft carries no
      number and the ledger does not think anything was received.
      verify: the payments list. The row menu should offer Open and Receipt;
              the receipt should open in the viewer with the number and the
              figures matching the row. Check a draft does not offer it.

- [x] `phonix-web` The logo, where the definition says it goes
      commit: "The mark at the top, and who is allowed to see it"
      `Logo` is a placement and a height - a height, because a wordmark is
      wide and squaring one is how a logo becomes unreadable. A workspace with
      none draws its name at the title's size rather than leaving a hole.

      The interesting part was the permission. `profile::load` requires
      `Settings`, so a report reading it would have drawn a letterhead for
      administrators and a gap for everybody else. `Letterhead` is a narrower
      answer to a narrower question - a name and a file id, no address - and
      its service takes a `Caller` and requires nothing of it, because that is
      already on every document a signed-in person can reach. The shell holds
      it and hands the kit a name and an image address, the way it already
      hands it the session, so `ui` learns nothing about organizations or file
      routes and the fetch happens once for the session.
      verify: set a logo in the workspace settings, then reopen the statement.
              It should appear where the definition places it, at a sane size.
              Clear the logo and check the name is drawn instead of a hole.

- [x] `phonix-web` A field that links to the document it names
      commit: "A number that opens what it names"
      `Field::link` takes a closure to the address beside the one to the
      value, and a drawn value is now the pair - what it says and where it
      goes. The address is the screen's alone: an export writes the words, and
      a printed link prints as the words it is made of. `None` from the
      closure draws plain text, which is the answer for a kind with no screen
      rather than a link to a page that is not there; all three kinds on a
      statement have one today, and a credit note opens at the invoice
      address because a credit note is an invoice row.
      verify: a statement. Click a document number: it should open that
              invoice or payment.

- [x] `phonix-web` Figures that line up, in a font chosen for them
      commit: "Numerals that stay in their column"
      `Field::figure` says a value is a figure; it sets it in tabular lining
      numerals and puts it at the end of its box, so a definition stops
      writing `Align::End` beside every amount. `Typeface` in
      `phonix_core::report` holds both halves of the choice - the stack the
      screen asks for and the base-14 name a PDF draws without carrying a font
      with it - because the writer cannot read a stylesheet. A figure in a
      band is a label at one end and a value at the other rather than a value
      after a label, which is what makes the ageing ladder and the four totals
      each line up down their own column. The ladder is the one place that
      says where it goes, because a figure otherwise goes to the right.
      verify: a statement. The amount and balance columns should have their
              decimal points in one line and every digit the same width, and
              the ageing figures and the totals should each line up under
              themselves.

- [x] `phonix-web` Printing prints the report, not the application
      commit: "The sheet is the page"
      The viewer carries its own `@media print`, and the rule is one
      sentence: anything that is not the sheet, does not contain it and is not
      inside it is not drawn. What is left of the chain down to the sheet
      gives up its width, its scrolling, its frame and its fit-to-width zoom,
      so a long report runs onto as many pages as it needs instead of being
      clipped by the box it was scrolling in. `@page` takes its size from the
      definition, which is how a landscape report prints landscape without
      anybody choosing it in the browser's dialog.
      verify: open a statement and press Print. The preview should show the
              statement alone, at its own size, with the navigation gone and
              nothing cut off the right-hand edge.

- [x] `phonix-web` The statement starts at a list of customers
      commit: "Whose account, before what it says"
      The statement is two screens now. `/accounting/reports/statement` is the
      customers, through the grid kit, and a row opens
      `/accounting/reports/statement/<party_id>`, which is the statement in
      the viewer. The dropdown is gone: the list is the picker. The grid reads
      `statement_customers` whole rather than paged, which is the read that
      exists and is bounded by the customer file - the same list the dropdown
      held. Back from a statement returns to the list rather than to
      Accounting, so the two screens read as one place.
      verify: `/accounting/reports/statement` on 3010 shows the customers.
              Click one: the statement opens, the span picker still works, and
              back returns to the list.

## Blocked

<!-- Items waiting on something outside the loop's reach. Each carries a
     `blocked:` line saying what it waits for. -->

## Done

<!-- The loop appends here with the commit sha. Newest first. -->

- [x] `phonix-web` The profit and loss, as a definition
      commit: "The last statement, and no second path"
      why: last of the four. With it there is one way of drawing a report in
           this codebase and no second path left to drift from it.
      touch: crates/phonix-web/src/ui/report/config/profit_and_loss.rs,
             crates/phonix-web/src/pages/sales/reports/profit_and_loss.rs
      done: `/accounting/reports/profit-and-loss` draws from a definition and
            its figures match for the same span. `pages/sales/reports/` now
            holds pickers and definitions only - no screen in it draws a report
            with markup of its own, and `shared.rs` keeps only what the toolbar
            controls still use.
      verify: all four statements, each in the viewer, each exported once as
              PDF. This is the last item, so it is also the whole engine's
              verification.
      stop: the queue ends here.

- [x] `phonix-web` The balance sheet, as a definition
      commit: "Three sections and two totals that agree"
      why: third of the four, and the first with real nesting - classes, the
           groups inside them, and a total that has to appear at both levels.
           If grouping cannot draw a balance sheet then grouping is not
           finished, and this is where that shows.
      touch: crates/phonix-web/src/ui/report/config/balance_sheet.rs,
             crates/phonix-web/src/pages/sales/reports/balance_sheet.rs
      done: `/accounting/reports/balance-sheet` draws from a definition, the
            as-at picker is a toolbar control and still works,
            `BalanceSheet::is_balanced` still decides the footer, the figures
            match for the same date, and the hand-written markup is deleted.
      verify: `/accounting/reports/balance-sheet` at a date you know. Check
              both levels of total, and that it still says whether it balances.

- [x] `phonix-web` The trial balance, as a definition
      commit: "The first statement onto the engine"
      why: second of the four statements. It is the simplest - every account,
           two columns, and a pair of totals that must agree - which makes it
           the one that says whether a statement can be expressed without a
           special case.
      touch: crates/phonix-web/src/ui/report/config/trial_balance.rs,
             crates/phonix-web/src/pages/sales/reports/trial_balance.rs
      done: `/accounting/reports/trial-balance` opens in the viewer and draws
            from a definition, `TrialBalance::is_balanced` still decides what
            the footer says, the figures match the hand-coded screen for the
            same span, and the hand-written markup is deleted.
      verify: `/accounting/reports/trial-balance` for a span you know, against
              the figures it gave before. The footer must still say whether it
              balances.

- [x] `phonix-services` The spreadsheet a job wrote
      commit: "A column somebody can add up"
      why: CSV loses the totals, the grouping and the type of every number -
           somebody who wanted to pivot the export has to retype it. This is
           the export an accounts department actually asks for, and the third
           writer on a path that by now has carried two.
      touch: Cargo.toml, crates/phonix-services/
      done: XLSX from the definition, with numbers as numbers and dates as
            dates rather than strings, a frozen header row, and group subtotals
            as real cells. It adds a pure-Rust dependency: name it in the commit
            body and say what it was chosen over. By this item the exporter has
            two writers and a printer: a format that is a *page* is the
            report's own page printed by a browser, and one that is not is a
            writer and a line on an enum. Adding a third writer is still that
            one line.
      verify: export as XLSX and open it. Sum a column in the spreadsheet - if
              the numbers are text it will not add up - and check the dates
              sort as dates and the header row is frozen.
      stop: seventh checkpoint. Every export format is in by here, and the three
            items after it are the remaining statements moving onto all of them
            at once.

- [x] `phonix-web` A group that opens and closes
      commit: "A section somebody has read"
      why: drill-down, and what makes a long grouped report readable - the
           groups are the report, and the detail is opened where somebody wants
           it. Without it a hundred-group report is a thousand-row scroll.
      touch: crates/phonix-web/src/ui/report/
      done: a group header toggles its detail where the definition allows it,
            the report opens in the state the definition names, and that state
            belongs to the browser - it does not survive a reload and never
            goes to the server. Printing and every export ignore it entirely:
            an archived statement with sections collapsed is evidence with
            holes in it.
      verify: collapse a group, reload, and confirm it opens in the state the
              definition names rather than the one you left it in.

- [x] `phonix-web` The chart, as a band the server drew
      commit: "A picture of the same numbers"
      why: charts are part of a report here, not a decoration on one, which is
           why this sits before the exports rather than after them. A
           JavaScript chart library is the wrong answer twice over: it draws
           nothing during the server's render, which is the hydration mismatch
           that takes the whole page down, and it draws nothing at all into a
           PDF.
      touch: crates/phonix-core/src/report/, crates/phonix-web/src/ui/report/
      done: a chart band drawn as inline SVG from the report's own rows, in the
            kinds a report actually needs - bar and column including stacked,
            line, area, and pie or donut - with axis, ticks, labels and legend
            identical on the server and in the browser. It reads the same rows
            and the same group subtotals the bands do rather than a query of
            its own, and a report whose only band is a chart is a report. No
            `<canvas>`, no chart dependency, no clock. The geometry is computed
            in `phonix_core::report` - not for a PDF writer, which is gone,
            but because it is the part a test can establish and the tests there
            can be run.
      verify: a report with each chart kind on it. Check the bars and the
              legend against the numbers in the table below them, then reload
              with the browser console open - a hydration mismatch shows there
              and kills every handler on the page.
      stop: fourth checkpoint. A chart that renders differently on the two sides
            is the failure this design exists to avoid, and it is only visible
            in a running browser.

- [x] `phonix-web` The pages a long report is read in
      commit: "Ten rows, and a way to the next ten"
      why: a list report is one endless sheet on screen today, and the viewer's
           toolbar has said since it was built that page navigation belongs
           there. Asked for directly on 2026-09-16, with the page size: **ten
           rows at a time by default**.
      touch: crates/phonix-core/src/report/paginate.rs,
             crates/phonix-web/src/ui/report/viewer.rs,
             crates/phonix-web/src/ui/report/render.rs
      done: **not as written.** `paginate` measured a printed page and the
            browser does that now, so there was nothing left for it to serve
            and it is deleted rather than given a second job. A screen page is
            a row count and nothing else. The toolbar carries
            previous and next with "page n of m" between them, the same two
            steps and the same words the grid's pager uses, drawn only where
            there is more than one page - and the report draws the rows of the
            page it is on rather than all of them.
      verify: the product list: ten rows, then Next through to the last page
              and back. The page count should not change when the look does,
              because ten is ten - but printing the same report should still
              break its pages by the sheet.
      stop: the toolbar is what was asked for and nobody has seen it.

- [x] `phonix-services` The writer that is not needed any more
      commit: "One less renderer"
      why: two PDF writers is the drift this decision was made to end, and the
           one that loses is the one that cannot see a stylesheet.
      touch: crates/phonix-services/src/report/pdf.rs, Cargo.toml
      done: `report/pdf.rs` and the `pdf-writer` dependency are gone, the CSV
            writer is untouched, and `phonix_core::report::paginate` stays -
            the viewer's page navigation is its caller now. Nothing names a
            band writer for a format a browser prints.

- [x] `phonix-server` The browser that prints
      commit: "The engine that draws the screen draws the file"
      also: a report's address had to become something a request could build,
            and the statement's span had to become something its address could
            say. A page that opened on its own default span would have printed
            the wrong months.
      why: the exporter has to turn a report into a PDF and the engine that
           draws it correctly is already on the machine.
      touch: crates/phonix-server/src/jobs.rs, crates/phonix-config/,
             config/base.toml
      done: an export of a PDF opens the report's own address in a headless
            browser and takes what it prints. The binary is named in config
            and validated at boot the way every other path is - a build with
            no browser fails fast rather than at the first export. One process
            per export, killed at a timeout, and a failure lands on the row
            with a reason like every other. The paper is the definition's,
            because `@page` already says so.
      verify: export the statement as PDF and put it beside the screen. The
              letterhead, the mark, the look, the rules and the totals should
              be the same document, not a resemblance.

- [x] `phonix-web` The token an export prints with
      commit: "A door that opens once, onto one page"
      moved: `phonix-web`, not `phonix-services`. The registry lives beside
             `AppState` because the minter and the reader are one process and
             the token never reaches a database.
      why: a browser fetching a tenant's report page is not signed in, and the
           export must be of what the person who asked for it may see. A
           session cookie cannot be borrowed and a permanent key would be a
           credential lying in a config file.
      touch: crates/phonix-services/src/auth/, crates/phonix-web/src/server/
      done: the exporter mints a token for the one request it is running -
            naming the tenant, the account, the report and its parameters, and
            good for minutes rather than hours - and the middleware accepts it
            as that account for that address alone. Used once, and never
            issued to anybody but the worker. A request with a stale, reused or
            mismatched token is refused the way an unauthenticated one is.

- [x] `docs` ADR 0008, and the writer it no longer describes
      commit: "The record catches up with the decision"
      why: section 9 says a report is drawn once and written by a format writer
           per format, and that is no longer what happens for a PDF. A decision
           this size is changed in the record before it is changed in the code,
           or the record stops being one.
      touch: docs/adr/0008-reporting.md
      done: section 9 says the PDF is the report's own page printed by a
            browser, what that costs - a binary on the box that runs the
            exporter, a process per export - and what it buys: one renderer,
            one stylesheet, and a chart, a logo and a script the base-14 faces
            never had. The band model stays what the screen and the CSV share,
            and the paginator stays what the viewer's page navigation reads.
            The status header names what is built.

- [x] `phonix-services` An export becomes a stored file
      commit: "Bytes nobody uploaded"
      An `exports` bucket, `files::generated::store`, and `exports::finish`
      and `fail`. The interesting half is what it is *not*: an upload arrives
      from outside, lands in quarantine and is inspected before anybody may
      have it back, and none of that applies to bytes a worker wrote out of
      rows this workspace already has. So the object goes straight to its
      stored key and the row is recorded `stored` - `record_generated` in one
      statement, and no `ON CONFLICT`, because two rows claiming one object is
      the bug the unique index exists to catch rather than something to
      swallow.

      The file's id is the request's, so a retry writes the same key instead
      of leaving two files - the reason `verify` derives its destination from
      the row it is working on. And the bucket takes no upload permission,
      because nothing uploads into it: what was allowed to *ask* was checked
      when the export was raised, against the report's own permission.

      `finish` takes no caller for the same reason - a worker has none, and
      three places to check one thing is how one of them ends up not checking.

- [x] `phonix-core` A report, rendered, and written out as CSV
      commit: "The shape that crosses a crate boundary"
      `Rendered` - the report's id and title, its look and page, and its bands
      as headings plus rows of text - and `writers::to_csv`, a plain function
      from one to a `String` with no pool and no caller anywhere near it. That
      is what lets the request path and the exporter call the same writer,
      which is the whole of ADR 0008 §9: the definition stays in `phonix-web`
      where its row type lives, and what crosses to `phonix-services` has its
      types erased.

      Two things the tests pin. Every row is padded to the report's own width,
      because a spreadsheet reading a short total row puts the figures under
      the wrong headings. And a cell is quoted when it starts with `=`, `+`,
      `-` or `@` as well as when it holds a comma, a quote or a newline - a
      cell reading `=1+1` is a document that computes something when somebody
      opens it, which is the injection every CSV export has to answer for.

      Cells are text for now. Enough for CSV and for a PDF, not enough for
      XLSX, and the answer when that lands is to move the grid's `Cell` into
      core rather than grow a second one - said in the module doc where the
      next person will be standing. There is no chart band to skip yet; the
      item that adds one adds the skip.

- [x] `phonix-core` The export request, as a row
      commit: "The row an export lives in"
      `core.report_exports`: which report, the parameters it was run with as
      JSONB, the format, who asked, when, one of four states, and the file or
      the reason there is not one. Two constraints carry the states' meaning
      rather than a comment - `ready` must have a file and `failed` must have
      a reason - so a worker cannot write a half-finished outcome and call it
      done. `requested_by` is `ON DELETE SET NULL` and the row says what that
      means: a worker renders as whoever asked, so an account gone between the
      request and the run leaves nobody to check and stops the render rather
      than anonymising it.

      The service raises one against **the report's own permission, passed in**
      rather than read off the request - a permission taken from a browser's
      request would be a caller naming the gate it is to be let through. And
      only the person who asked can read a request back: how far somebody
      else's document has got, and which file it landed in, is theirs.

      `phonix_services::report` is a new module in that crate, which ADR 0008
      §9 asks for by name: the writers live there and the request half is
      what they are called from.

      Validated against `viaba_tenant_med_app_staging` in a rolled-back
      transaction - the table and both indexes created, a row written through
      the same shape the service uses, and four refusals checked at a
      savepoint: an unknown format, an unknown state, a `ready` with no file
      and a `failed` with no reason. **It is applied now**: the watcher
      rebuilt mid-item and the boot sweep ran it, so the tenant is on
      `core:0025` and this migration is frozen.

- [x] `phonix-db` A declared default never reaches a workspace that is current
      commit: "The insert that failed every boot, quietly"
      **The queued diagnosis was wrong and the log said so.** The tab was
      empty because `install_from_config` wrote `INSERT INTO
      document_settings` on a connection whose search path is the *app's*
      schema, so it did not resolve - and that failed the whole tenant
      migration, on every boot, after the apps before it had been dealt with.
      `tenant migration failed: relation "document_settings" does not exist`
      had been in `var/development.00*.log` since the installer landed.
      Qualified now, the way `apps::record_installed` already spells out
      `core.installed_apps` and says why.

      The fingerprint hole is real as well, and is fixed with it: the sweep
      skips a tenant whose `schema_fingerprint` matches, and that fingerprint
      was migration versions only, so a series or a document added to an app
      every workspace already has would reach none of them. It now carries a
      digest of `config/numbering` and `config/documents`. That was **not**
      what emptied the tab, and the entry that said so has been corrected
      rather than left standing.

      Proved against the real tenant in a rolled-back transaction: the
      unqualified statement gives exactly the logged error under
      `search_path = books`, the qualified one writes, and the six declared
      documents land.

- [x] `phonix-config` The document settings each app declares
      commit: "The question the app asks about a document"
      `config/documents/<app_id>.toml` beside `config/numbering/`, read on the
      same install pass and inserted `ON CONFLICT DO NOTHING`. A document type
      declared here that the app issues no series for is refused at start-up,
      which is the cross-check the item asked for and the reason the loader
      reads both files. **No header or footer text in the file**: those are the
      tenant's own words, and a default here would be this codebase writing
      English sentences onto every document a workspace issues. Books declares
      its invoice, credit note and receipt; Inventory declares the three
      documents that leave the building and not the three that do not.

      Checked against `viaba_tenant_med_app_staging` in a rolled-back
      transaction: the `UNNEST` insert put three rows in, one of them with no
      mark at all, and a second pass with different values inserted nothing and
      changed nothing - which is the whole point of the conflict clause.

- [x] `phonix-services` The document settings have no trail
      commit: "Who changed the invoice footer"
      A `DOCUMENT_SETTINGS` kind, in `ENTITY_KINDS` so `find_kind` resolves it
      and a history renders it, and `save` recording a `{from, to}` the way
      `settings::save` does. Keyed by the document type rather than by a
      settings singleton, because the question asked after an invoice goes out
      wrong is about the invoice - a workspace that only ever edited that one
      does not read receipt noise to find when.

- [x] `phonix-core` The document settings a tenant keeps
      commit: "Answers, and the column that refuses a layout"
      `core.document_settings` keyed by document type: the look, the paper and
      which way up, the mark as a band-edge-height triple that is all present
      or all absent, and the tenant's own words at the head and the foot. The
      look column names the three and refuses a fourth, which is the back door
      a report designer would come through. `DocumentSettings` in
      `phonix_core::report` validates its own text so a form is told which box
      is wrong rather than a constraint name, and a type with no row **is** the
      defaults rather than an `Option` every caller has to remember. The
      service splits the way `profile` does: the screen's read is gated on
      `Settings`, the read a report does while drawing is not.

      **Validated against `viaba_tenant_med_app_staging`**, which was on core
      0023, in one transaction that was rolled back: the table created, two
      rows written - one with a mark and one without - and four refusals
      checked at the savepoint, an unknown look, a half-set mark, an unknown
      band and a footer over 500. The database is as it was.

- [x] `phonix-web` The viewer, which fills the page inside the shell
      Verified in the running application on 2026-09-16, with two faults
      sent back as items: print, and where the statement starts.
      commit: "The frame, and the twenty toolbars it prevents"
      The frame, its toolbar and the surface the sheet sits on. The parameters
      a report takes are controls on that toolbar. Fit-to-width is real and
      measured - a zero-height gauge beside the sheet, `zoom` on the sheet,
      and the measurement runs in the browser after the first paint so both
      builds render the same markup. Print is `window.print()` with a
      server-side no-op beside it, the way `ui/clipboard` already does it. The
      export menu is drawn only when the definition registers a format, and
      none does yet. Page navigation is deliberately absent: nothing counts
      pages until the paginator lands, and a control that always said "1 of 1"
      would be a promise.

- [x] `phonix-web` The customer statement, as a definition
      Verified on 2026-09-16: the figures are right and the frame is right.
      commit: "What the first real document said about the model"
      The first document through the engine, and it found the model wrong -
      which is what this item was for. A report is now drawn from **one
      value**, not a vector: the bands read that value, and the detail band
      reads a sequence inside it through `Band::lines`, which takes the line
      type's own `Field<L>` closures and erases them so the headings and the
      cells under them cannot fall out of step. A letterhead showing a
      customer above rows that are that customer's lines is impossible any
      other way.

      The statement is `Professional` on a wider page - a look now carries its
      margins onto the page, so "wider margins" is a fact rather than a
      sentence. The ageing ladder is the footer's left column and the four
      totals are its right. Two deliberate losses: the foreign-currency
      sub-line under a document number is now a parenthetical beside it,
      because a band draws one line per row; and an empty statement shows an
      empty line area rather than the words "Nothing to show", which is what a
      printed document does.

- [x] `phonix-web` A report drawn from its definition
      One component, and the sheet it draws is the size it will print at -
      the metrics reach the markup as `mm` and `pt` in a `style`, which CSS
      understands as well as a PDF does, so the three looks are one set of
      bands measured differently rather than three sets of markup. Classes
      say only which colour a rule or a heading is.

      Three things decided while drawing it. A band outside the detail reads
      the **first** row, which is a document's single record and nothing at
      all for an empty list. The detail band's own labels are the heading
      row, so a report does not declare its columns twice. And the group
      bands are drawn once, around the rows - the shape is right and the
      count is what grouping adds.

- [x] `phonix-web` The report definition, bound to a typed row
      `ui/report/` beside `ui/table/`, and the same arrangement: a module
      contributes a value and the kit draws it. A `Field<T>` is a closure over
      the row and a `Band<T>` is a kind from `phonix_core::report` with fields
      in it, so a field that names nothing does not compile - which is the
      whole reason a definition is Rust. `Field::text` is a constant dressed
      as a field, which is how a title and a column heading are one mechanism
      rather than two.

      **A definition holds no query.** Rows are handed to the renderer by the
      screen, from a server function that already exists. And `Extent` is the
      export declaration: a report **grows** until it says otherwise, because
      a report wrongly called bounded holds a connection open for as long as
      it takes, while a job that need not have been one is only slower.
      `bounded_by` takes the sentence saying what bounds it.

- [x] `phonix-core` The three looks, as measurements rather than styling
      `ReportTheme` resolves to a `Metrics` of type sizes in points and
      everything else in millimetres: band heights one per kind, cell padding,
      and a rule weight for each of the five places a rule can go. **Zero is
      how a look says it draws no rule there** - Modern separates columns by
      alignment, Compact rules every edge - so there is no second field
      saying whether the first one counts. Colour is how far colour may
      reach, not which colour it is: the hue belongs to the screen's palette
      and to the document, and a crate that compiles to wasm has no business
      holding either. Margins are the look's, since Professional's wider page
      is part of what makes it the document look, and a document setting
      still overrides them.

- [x] `phonix-core` The band model, and the page a report is printed on
      `phonix_core::report`, in two files: the seven band kinds with
      `ReportKind` and `Align` beside them, and the sheet - paper, orientation,
      margins, and what is left for bands once the margins are off it.
      Millimetres throughout, because the PDF writer works in them and a
      stylesheet can be given whatever unit it likes. `LogoPlacement` names a
      band and an edge, and *whether* there is a logo is the `Option` around
      it rather than a state inside it. The report's `Align` is its own type:
      the grid's carries a CSS class and belongs to the screen, and this one
      is read by a writer that has no CSS.

- [x] `docs` ADR 0008, the reporting engine
      Twenty-eight items of design, written down before any of it is built.
      The load-bearing part is section 2: `phonix_core::report` is a new
      top-level module in the crate that is meant not to grow one, and the
      reason is that the screen and the PDF writer are consumers of the same
      band model from different crates. Section 3 states the denial - a
      definition reaches no datasource of its own and carries no SQL - and
      section 6 draws the line between a layout, which is code, and a
      document setting, which is the tenant's, with the report designer
      refused by name. 0006 gained a pointer at it.

- [x] `phonix-web` Recruitment, on a screen
      The last of the People screens. Hiring is a button beside the form
      rather than a stage on the picker, which is what `check` refusing
      `hired` was for: it asks for a start date, opens the engagement, and
      says which record it landed on. `hire` gained a `Hired { employee_id,
      rejoined }` so the screen can tell somebody they did *not* create a
      second person - silence there reads as "new" and is wrong half the
      time. A vacancy now carries its open-application count beside
      `filled`: a role nobody holds and nobody wants is a different problem
      from one with four people waiting on an answer.

- [x] `app-hr` Recruitment against the vacancies that already exist
      An applicant is its own row, not an employee with a flag: most never
      become one. The piece worth reading is `hire`, which looks for an
      existing employee on the work email and opens a **second engagement**
      where it finds one - somebody who left in 2019 and applies again is
      one person with two periods of employment, which is the
      duplicate-identity case 0002 was written to refuse and the one a
      recruitment module walks into first. Matching is on the address only;
      guessing at names would merge two people called the same thing.
      Hiring is refused as a typed stage - it opens an engagement, and a
      form that reached it by picking a word would open none.

- [x] `phonix-web` Movements, on a screen
      One address, two screens: the status decides whether a draft's form or
      the confirmed document is drawn, so a link somebody sent last week
      still opens what they meant. Confirming can refuse - a promotion that
      moves nobody - and the refusal is shown rather than swallowed. The
      employee record gains a Movements tab: the audit trail says which rows
      changed, this says what was decided.
      Also removed a `today` parameter from `EmployeeInput::from_employee`
      that the shift field had made dead - it fed a struct update that no
      longer had a field left to fill.

- [x] `app-hr` The employee lifecycle documents
      One table and three kinds - promotion, transfer, exit - for the reason
      `invoices.kind` holds a credit note. Confirming writes the rows
      through `employee::move_to` and `employee::record_leaver` rather than
      writing them a second way, so the rules about how an assignment closes
      stay in one place. Drafted freely, gated at confirm, numbered there
      too. Onboarding is deliberately not a fourth kind: hiring already has
      a document, and a movement recording somebody's arrival would have
      nobody to point at.

- [x] `phonix-web` Shifts, on a screen, and lateness on the timesheet
      The shift list and form, the picker on both employee forms, and the
      column the shift was built for. Lateness needed the workspace zone, so
      `chrono-tz` joins the workspace as a **server-only** dependency - the
      user chose this on 2026-09-15 over comparing in UTC, which would have
      been quietly wrong by the offset everywhere but London. An unresolvable
      zone name falls back to UTC with a `tracing::warn!`, which is what
      `phonix_core::locale::timezone` says should happen. The verdict is
      absent rather than invented where either half is missing: no shift on
      the assignment, or no check-in time on the record.

- [x] `app-hr` Shift types, and the roster that assigns them
      The model, as with the two before it. A shift carries its hours and
      two grace windows - Frappe HR has both and they are not the same
      number, since five minutes late is traffic and five minutes early is a
      decision. Which shift somebody is on is a dated assignment row like
      their department and their calendar. `arrival` and `departure` are
      pure and take a local clock time, so the grace window means something
      rather than sitting in a column nothing reads. A night shift ends
      before it starts and that is allowed: refusing it would refuse the
      case a grace window is most often used for.

- [x] `phonix-web` Attendance, on a screen
      One screen rather than two: a person, a month, and every day of it
      resolved - keying a day, reading the month and correcting a record are
      the same act from the reader's side. All seven `DayOutcome` answers are
      drawn, `NotRecorded` and `Unknown` in warning tone rather than as
      blanks, because a blank reads as "fine" and means "nobody knows".
      `TimesheetDay` moved from phonix-services to app-hr on the way: a
      server fn's return type has to compile for wasm, and phonix-services
      does not.

- [x] `app-hr` Attendance, as what was recorded rather than what was expected
      Unblocked by the user on 2026-09-15: **no geolocation**, so a record
      says who was here, when, and which device or person asserted it, and
      nothing about where anybody was standing. The model, not the screens -
      those are the item above. `DayOutcome::resolve` is the piece worth
      reading: a day off is not an absence however the record reads, coming
      in on one is its own answer because overtime is derived from it, and
      neither silence nor a missing calendar counts as an absence.

- [x] `phonix-web` The holiday calendar, on a screen
      A list, a form with its days, and the weekly-off generator, which is
      the part anybody uses - nobody types fifty-two Saturdays. Generating
      replaces the generated rows and leaves the named ones, so pressing the
      button twice is not an error the screen cannot explain. The calendar is
      a picker on both the new-hire form and the move form, so
      `assignments.holiday_list_id` fills through the dated row like every
      other fact about somebody, and `AssignmentInput::next` carries it -
      a promotion in place does not take somebody off their calendar.

- [x] `app-hr` The holiday calendar, which leave cannot be counted without
      The model, not the screens - those are the item above. `holiday_lists`
      and `holidays` with a span, `assignments.holiday_list_id` so which
      calendar applies is dated like everything else about somebody, and
      `current_staff` replaced to carry it. `WorkingDay` is three answers and
      not a boolean: a date the calendar does not cover is not a working day,
      which is what stops an empty calendar reading as a full working year.
      The SQL is unverified against a live database - no tenant is applied
      here, so the migration and the queries are checked by the compiler only.

- [x] `phonix-web` What has been credited, on the invoice itself
      Third of three, and the trio is closed. A posted invoice that has been
      credited or paid carries three more rows under its total - credited,
      settled, what is left - and links to the notes themselves. Fetched on
      its own call rather than hung on the invoice, the way the journal link
      already is: the document is what was raised, this is what happened to
      it since. Nothing is drawn on a draft or on an invoice nobody has
      touched.

- [x] `app-books` What an invoice has been credited, and what is still owed
      Two of the three. The statement, the ageing, money on account, the front
      page and both settlement guards now treat a credit note as money off:
      `EntryKind` has a third variant, what is left on an invoice is its gross
      less allocations less credits everywhere that figure is worked out, and
      a credit note is no longer offered as something to pay - which it was.
      The third, showing it on the invoice, is its own item above: it is a new
      query and a panel rather than more of this arithmetic.

- [x] `phonix-core` The catalogue parity tests, somewhere they can be run
      Three, not two - the placeholder check uses the same helper and came
      with them, into `i18n/catalog.rs`, which is the module that documents
      the overlay. They run in ten seconds now instead of being killed for
      memory, and running them confirmed the Chinese catalogue above. The
      note asked whether file I/O in a `#[cfg(test)]` module offends the
      wasm rule: it does not. `build.rs` already reads `i18n/en.json` on the
      host, the crate exempts tests from its own denies, and
      `cargo check --all-targets --target wasm32-unknown-unknown` passes.

- [x] `phonix-web` The Chinese catalog, thirty-six keys behind
      Thirty-seven, in the event. `locales/zh.json` now carries every key
      `en.json` does. The acceptance test could not be run: building
      phonix-web's test binary is OOM-killed on this machine, so parity was
      verified by script instead - both directions, plus every `{placeholder}`
      matching English across all three overlays. Making that test runnable
      is the item above.

- [x] `phonix-core` The permission root that still says Sales
      `Pages.Sales.*` is `Pages.Accounting.*`, Books' home is `/accounting`,
      and migration 0023 rewrites the prefix in `role_permissions` and
      `user_permissions` so stored grants survive - without it every grant
      under the old root would be pruned on load rather than rejected, which
      is a silent loss of access. `identity_events` keeps the old names: it
      records what a permission was called when somebody changed it.

- [x] `phonix-web` The routes that still name the crates
      `/accounting` for the chart, journals, periods and the four statements;
      `/selling` for the order, the delivery, the invoice and the payment,
      which is the first namespace two app crates answer in. Books' own home
      stays at `/sales`: a test ties an app's home to its permission root, so
      that one address moves with `Pages.Sales`, queued as its own item.

- [x] `phonix-web` The menus name the crates, not what somebody is doing
      The decision the entry asked for: a top-level Selling holding the
      whole sell-side chain - order, delivery, invoice, payment - and a
      top-level Accounting holding the chart, the journals, the periods and
      the four statements. Invoices sit under Selling only. Selling is
      ungated because it spans two apps. Labels and grouping only; the
      routes still read `/sales` and `/inventory`, queued as its own item.

- [x] `app-books` Raising a credit note against an invoice
      Second of three. `?credits=<id>` on the invoice form, and a button on a
      posted invoice. Crediting part of one is deleting the lines that are not
      coming back - the same edit as any other draft rather than a second way
      of saying it. Voiding and crediting are both offered and are not
      alternatives.

- [x] `app-books` The credit note, as a kind of invoice
      First of three. The item said a numbering series was already reserved;
      it was not - `credit_note` appeared only in a doc example and a test
      fixture, and `config/numbering/books.toml` declared three series. Now it
      declares four. A credit note is an invoice with `kind = credit_note`
      rather than a table of its own, which is what ERPNext and Odoo both do.

- [x] `phonix-web` Choosing which list a customer is on
      The last gap in the price-list chain: lists, prices, customers and the
      line that opens on them all work from the application now. A tab rather
      than a field on the party form, because `PartyInput` is master's type and
      master may not learn what a price list is.

- [x] `phonix-web` Keeping the price lists
      Last of three. A list and its prices are one screen, not two - the shape
      a sales order has. Grid, editor, routes and a nav entry. Found on the
      way: assigning a customer to a list still has no screen, which is queued
      above and is the last thing standing between the tables and the feature.

- [x] `phonix-web` Re-pricing a line when its quantity crosses a break
      On the quantity's `change`, so one request per edit rather than one per
      keystroke. A price is replaced only while it is still exactly what the
      list last quoted into the box; the moment somebody edits it, it is theirs
      - checked again when the answer comes back, because the request takes
      long enough for somebody to type into the box during it.

- [x] `app-inventory` A customer's price list, and the line that opens on it
      Second of three. `party_price_lists` holds a bare party id, because a
      price list is a selling fact and master should not learn what one is.
      Found on the way: the ADR and the service both said a line opened on
      `items.sale_price`, and neither the picker nor the handler ever did that
      - every price was typed. Both corrected.

- [x] `app-inventory` Price lists, and which price wins
      First of three the price-list item split into. Inventory, not Books - the
      hint said `app-books/src/pricing.rs` and that module is line arithmetic
      which never reads an item price. Tables, model and the resolution rule
      with seven tests; nothing is wired to it yet.

- [x] `phonix-web` Raising an invoice against a despatch
      Against a delivery rather than a customer - the mirror of
      `bill::against_order`, and what somebody does with a despatch note in
      hand. `Deliveries` grew a read for it, because Books may not look at
      delivery lines itself. A line with no order behind it comes back with no
      price rather than a guessed one.

- [x] `app-books` The invoice line that names a delivery line
      Last of three. `invoice_lines.delivery_line_id` is a bare id with no
      foreign key, posting raises the `Deliveries` port before it commits, and
      a refusal rolls the invoice back to a draft. WORKFLOWS.md's entry is
      ticked. No picker on the screen yet - queued above.

- [x] `phonix-ports` The `Deliveries` port
      Second of three. The first port that runs Books -> Inventory rather than
      the other way. One call takes every line, locks each as it reads it, and
      refuses the whole set rather than letting two invoices race for the same
      quantity. ADR 0006's port table names it, and the two passages saying the
      link does not exist now say which third of it is left.

- [x] `app-inventory` What a delivery has had invoiced against it
      First of three the invoice-bills-a-delivery item split into: it needed a
      migration, a port and a Books change, which is three commits. This is
      Inventory's half - `delivery_lines.invoiced`, the aged
      `uninvoiced_deliveries` view, and the read - mirroring `receipt_lines.
      billed` and `unbilled_receipts` exactly. Migration validated against
      viaba_tenant_med_app_staging in a rolled-back transaction.

- [x] `phonix-web` A grid that opens already narrowed
      `Filter::opening_on` declares it, and `GridState` and `initial_request`
      seed it from one shared function so the two cannot disagree. The staff
      list opens on current staff; every other grid still opens on everything,
      and their `default_value() == ""` tests are untouched.

- [x] `workspace` The workspace adopts rustfmt
      Decided: adopt rather than drop the gate. `cargo fmt --all` in one sweep,
      206 files, no `rustfmt.toml` - default rustfmt is what the gate runs and
      measuring showed no width makes this code a no-op anyway. `check.ps1`'s
      fmt gate passes from here, and a scoped `cargo fmt -p <crate>` now touches
      only what was just edited.

- [x] `phonix-web` The chart of accounts grid, last of the four that grow
      All four are now paged. Class and postable are derived from the account
      type rather than stored, so the store sorts and filters them through
      expressions generated from `AccountType::ALL` - the rule stays in the
      enum rather than being copied into SQL.

- [x] `phonix-web` The agreement tests for the users and locations grids
      Both now assert that every column offering a sort or a search is one the
      store actually handles, and the locations grid asserts every kind it
      offers can be bound. Typechecked, not executed - the `phonix-web` test
      binary is OOM-killed on this machine.

- [x] `phonix-web` The employees filter that opened on a choice it did not apply
      "All" is first now, which is what the screen already did, and the grid
      carries the agreement tests its neighbours have. Opening genuinely
      narrowed needs a capability the kit does not have; queued rather than
      faked.

- [x] `phonix-web` The locations grid, third of the four that grow
      The one tree among them. Tree order and depth now come off the stored
      path via `string_to_array`, so a page can be drawn without the rows
      above it; `in_tree_order` stays for the forms, which still read the
      whole tree. Both filters moved to SQL, `on_hand` mapping to `internal`.

- [x] `phonix-services` `directory::find`, deleted rather than fixed
      The item said a user's screen read the whole table through it. That was
      wrong: nothing called it at all, and `directory::card` already reads one
      account by id through `store::card`. So the whole-table read is gone by
      deletion, and no second single-row reader was written.

- [x] `phonix-web` The users grid, second of the four that grow
      Search and sort answered in SQL; the count shares `WHERE` with the
      select, and the role predicate is an `EXISTS` so searching one role
      still shows every role a row holds. `directory::list` stays: the REST
      API has four callers of it.

- [x] `phonix-web` The employees grid, first of the four that grow
      The staff list pages server-side: search, both filters and the sort are
      answered in SQL, and the count shares `FROM` and `WHERE` with the select.
      The unpaged `employee::list` is gone at all three layers; the manager
      picker's `employed` is untouched.
