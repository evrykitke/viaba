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
```

Only `why:` is required. `touch:` is a hint, not a fence — the loop may find
the work lives elsewhere and will say so. An item with a `blocked:` line is
skipped until that line is removed.

Keep items small enough that one of them is one commit. If an item needs three
commits it is three items.

---

## Next

> **Read this first.** The loop paused for browser verification after
> `0ea7fd6` and has since resumed. Nothing is ever half-done at a stop:
> the tree is clean and every item below is untouched.
>
> **Nothing in this branch has run against a database.** Migrations
> `hr/0003` (holidays), `hr/0004` (attendance), `hr/0005` (shifts),
> `hr/0006` (movements), `hr/0007` (applicants) and
> `core/0023` (the `Pages.Sales` → `Pages.Accounting` rename) are all
> unapplied and compiler-checked only. The `generate_series` + `LATERAL`
> queries in `hr::holiday::working_days` and `hr::shift::for_span` are the
> least-exercised SQL in it. Expect the first browser run to be where that
> gets found out.
>
> **Decisions the user made during the run**, all already applied:
> Selling/Accounting menus with invoices under Selling only; the permission
> root renamed with a migration rather than dropping the home-to-permission
> invariant; no geolocation on attendance; `chrono-tz` as a server-only
> dependency for lateness.
>
> **The People section is one item from finished.** Calendar, attendance,
> shifts and the lifecycle documents are built end to end; recruitment has a
> model and a gated service and no screen. After that the section holds only
> expense claims (needs `app-books`), performance management and timesheets,
> none of which are queued yet.

> **The People section is finished**, against what was queued on
> 2026-09-15. What Frappe HR has and viaba does not is now only: expense
> claims (needs `app-books`), performance management, and timesheets - plus
> leave, payroll, salary and contracts, which are deliberate omissions with
> their reasoning in ADR 0006 §9. None of these are queued; adding one is a
> decision rather than a gap. See `WORKFLOWS.md`.

## Blocked

<!-- Items waiting on something outside the loop's reach. Each carries a
     `blocked:` line saying what it waits for. -->

## Done

<!-- The loop appends here with the commit sha. Newest first. -->

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
