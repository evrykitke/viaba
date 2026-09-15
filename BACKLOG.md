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

- [ ] `app-inventory` A customer's price list, and the line that opens on it
      why: second of three. The resolution exists and nothing calls it - a
           sales order line still opens on `items.sale_price`, so a wholesale
           customer and a walk-in are still quoted one number.
      touch: crates/phonix-services/src/inventory/sales_order.rs
      done: a party carries a default price list, a sales order resolves each
            line through it at the order's date and quantity, and falls back to
            `sale_price` only where no list prices the variant. ADR 0006's
            "there is no price list" paragraph changes in the same commit.

- [ ] `phonix-web` Keeping the price lists
      why: last of three. The tables exist and nothing can put a price in one
           but a migration, so the feature is unreachable from the application.
      touch: crates/phonix-web/src/ui/table/config/, pages/inventory/
      done: price lists are listed, created and edited, and a variant's prices
            in one list can be added with their break and their window.

- [ ] `app-books` The credit note, which has a numbering series and nothing else
      why: `phonix-config/src/numbering.rs` already reserves a credit-note
           series, so the gap is visible from the configuration alone. A sales
           return currently has no document that reverses the revenue.
      touch: crates/phonix-config/src/numbering.rs, crates/app-books/src/invoice.rs
      done: a credit note posts against an invoice, reverses the revenue and
            tax lines, and the invoice shows what has been credited.

### People — the Frappe HR revamp

Read `WORKFLOWS.md` before starting any of these, and obey the dated-assignment
rule: what changes about somebody is an `assignments` row, never a column on
`employees`.

- [ ] `app-hr` The holiday calendar, which leave cannot be counted without
      why: Frappe HR makes the regional holiday list the thing leave, attendance
           and payroll all count against. It is the only piece of the people
           revamp with no dependency of its own, so it goes first.
      done: a workspace has named holiday lists with dated entries, an employee
            resolves to one through their assignment, and a date can be asked
            whether it is a working day.

- [ ] `app-hr` Attendance, as what was recorded rather than what was expected
      why: check-in and check-out is the input every other HR number is derived
           from, and Frappe HR treats it as its own record rather than a
           side-effect of a shift.
      touch: crates/app-hr/src/ — a new module beside employee.rs
      done: an employee has dated attendance records, a day resolves to
            present/absent/half-day against the holiday calendar, and the
            record says which device or person asserted it.
      blocked: geolocation check-in is in the Frappe docs and is a privacy
               decision, not a technical one. Say whether you want it before
               this is taken.

- [ ] `app-hr` Shift types, and the roster that assigns them
      why: attendance without an expected shift can say somebody was present
           but not whether they were late, and Frappe HR separates the two for
           exactly that reason.
      done: shift types carry start, end and a grace window; an employee's
            shift is a dated assignment row like every other assignment.

- [ ] `app-hr` The employee lifecycle documents
      why: the dated chain underneath onboarding, promotion, transfer and exit
           already exists — engagements and assignments. What is missing is the
           workflow on top: a promotion today is a hand-written assignment row.
      done: a promotion or transfer is a document that writes the assignment
            rows, and an exit writes the engagement end with its reason from
            the existing list.

- [ ] `app-hr` Recruitment against the vacancies that already exist
      why: job positions are rows precisely so that "what are we recruiting
           for" has a query, and nothing yet answers it. `filled` is already
           counted over open assignments.
      done: an applicant applies against a job position, moves through named
            stages, and a hire opens an engagement rather than duplicating the
            person.

## Blocked

<!-- Items waiting on something outside the loop's reach. Each carries a
     `blocked:` line saying what it waits for. -->

## Done

<!-- The loop appends here with the commit sha. Newest first. -->

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
