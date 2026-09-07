# ADR 0006 — Apps, ports, and the data a workspace starts with

Status: proposed; built so far — section 1 (the enablement filter), section 2
in full (`phonix-ports`, `CostCentres`, and now `Ledger`), section 3 (generated
codes, for HR and for item codes), section 4 in full (sensible defaults, the
exhaustive chart of accounts, and the setup checklist), section 5 in full (the
general ledger: double entry enforced by the type, append-only posting, sourced
journals, period locks, dimensions on the line, and the six-column currency
snapshot), section 9 (the HR app), the first half of section 7 — the Inventory
app's vocabulary: items, variants, categories, units, locations and warehouses,
each with its screens, and the item's account mapping resolved through the
`Ledger` port — and now **the stock ledger beneath the documents**: lots and
serials, stock moves, quants, valuation layers, the on-hand and movement
screens, and the adjustment. With it, 6.1 (valuation posts as stock moves), 6.6
(negative stock refused) and 6.7 (sub-ledger to general ledger by `GROUP BY`)
hold in code rather than on paper. See *The stock ledger, as built* under
section 7.

Still specified only — the **documents** of section 7. Requisition, consolidated
requisition, purchase order, receipt, bill and transfer. Each of them is now a
form and a header over `stock::apply`, which is the whole reason the ledger came
first.

`Stock` is still not declared, as section 2 says it should not be: Books does
not yet put cost of goods sold on an invoice, and that is the caller the port
waits for.

## Section 7 follows Odoo

Asked for by name, and taken seriously. The seven **location types**, stock as
**double entry between two locations**, warehouses owning a view node and a
stock location, 1/2/3-step receipts and deliveries, UoM categories, costing and
valuation on the item **category**, goods-versus-service with a tracking flag,
lots and serials with expiry, and **variants** with stock held against the
variant rather than the item — all of it is Odoo's model, because that model is
what makes stock reconcile by arithmetic rather than by a nightly job.

Two departures, both deliberate:

* **Item codes are generated.** Odoo lets somebody type one; section 3 of this
  record says a code that a person invents is a code that collides. The
  *barcode* stays hand-typed, for the reason section 3 gives.
* **Account overrides do not name an account across the app boundary.** Odoo
  hangs six accounts off a product category. So does this — as a bare
  `account_id` with **no foreign key**, resolved through the `Ledger` port and
  verified by it on every posting, exactly as `books.invoices` carries a
  `master.parties` id. Section 2's rule is not negotiable for a convenience.

Date: 2026-09-04

ADR 0001 drew the line between infrastructure and an app, and proved it with
one app that issues one document. This record is about what happens when there
are six of them.

It answers four questions 0001 left open, because with a single app above
`master` none of them had a wrong answer yet:

1. **How does one app use another?** 0001 says "resolve through a capability
   port" twice and never says what a port is.
2. **What does switching an app off actually do?** Today it deletes permission
   grants. That is not a toggle; it is a demolition with an undo button that
   does not work.
3. **Where does a code come from?** `master.parties.code` is typed by a human.
   An item code cannot be, and an item's *UPC* must be.
4. **What is in a workspace on the first morning?** Today: nothing. A chart of
   accounts nobody has entered is an accounting system nobody can post to.

And it specifies the two apps that come next — Books proper, and Inventory —
against what the incumbents in this market actually get wrong, because "an
accounting package" is not a design and the gaps are where the design is.

---

## 1. Three axes, and the two that are currently fused

There are three independent facts about whether a screen appears, and the
codebase currently has two of them stored in one place.

| Axis | Decided by | Where it lives | Changes how often |
| --- | --- | --- | --- |
| **Compiled in** | the deployment | `apps::CATALOG`, `tenancy::APPS` | per release |
| **Enabled** | the workspace | `core.installed_apps.enabled_at` | rarely |
| **Permitted** | the role | `core.role_permissions` | constantly |

`workspace::apps::uninstall` implements *enabled* by writing to *permitted*: it
calls `role::revoke_everywhere(app.permission)`, which deletes every grant
beneath the app's root from every role in the workspace, including roles the
organization wrote themselves, and from every per-user override. Its own doc
comment is honest about the consequence — switching the app back on "leaves the
custom roles for somebody to decide about again".

That is the wrong shape, and the argument that it is honest is an argument about
a problem that should not exist. A subscription lapsing for a month must not
cost an organization the permission structure they spent a week building. And
the failure is silent in the direction that matters: nothing tells the
administrator that turning Books off just erased eleven grants across four
roles.

### The rule

> **Enablement is a filter over the effective permission set, not a write to
> it.** Grants are what a role holds. Enablement is what the workspace has
> switched on. Neither may edit the other.

Concretely, and this is the whole change:

```rust
// phonix_services::identity::authentication::load_auth_user
let permissions = permission::resolve_for_user(pool, account.id)
    .await?
    .for_enabled_apps(&installs::enabled_ids(pool).await?);
```

`PermissionSet::for_enabled_apps` already exists — `permission_set.rs:54` — and
is already used to seed the static roles. Applying it one layer later is the
entire mechanism. `uninstall` then stops calling `revoke_everywhere` and
`sync_static_roles` altogether, and switching an app off writes exactly one
column.

### What this buys, and what it deliberately does not

It is not a cosmetic toggle. A disabled app is gone from the navigation, the
launcher, the command palette and every grid — all of which already answer to
permissions — *and* its services still refuse, because `Caller::require` reads
the same filtered set. An app a workspace has not paid for cannot be reached by
guessing its URL, and cannot be reached through the public API either.

What it stops doing is destroying anything. The grants sit in
`core.role_permissions` untouched; the filter is applied per request, at the one
place an `AuthUser` is assembled. Re-enabling is instantaneous and complete,
including for custom roles and per-user overrides.

This also means enablement needs no second gate anywhere. The reason
`workspace::apps` gave for revoking — "a second gate beside permissions would be
two things to keep in step, and one of them would eventually be forgotten in a
service nobody thought about" — is correct, and is exactly why the filter goes
where the permission set is *built* rather than where it is checked. There is
one call site, and a service that forgets to consider enablement is not
possible, because no service considers it.

### The store shows everything; the menu shows what is on

Every app compiled into the build is listed in `Administration → Apps`,
including the ones this workspace has never switched on — otherwise there is
nowhere to switch them on *from*. The navigation shows only what is enabled.
Those are different screens answering different questions and neither is a view
of the other.

---

## 2. An app depends on another only through a port

0001 forbids the foreign key and names the alternative without defining it.
Here is the definition.

> **A port is a trait, declared by the app that needs the capability, in a crate
> that neither app owns.** The app that provides the capability implements it.
> Nothing else crosses the boundary — not a type, not a table, not a query.

The direction matters and is the part that is easy to get backwards. Inventory
needs to post a journal when stock is received. It does **not** depend on
`app-books`. It declares what it needs:

```rust
// crates/phonix-ports/src/ledger.rs
#[async_trait]
pub trait Ledger {
    /// Post a balanced journal, or refuse the whole of it.
    ///
    /// Returns the reference the ledger filed it under, which the caller
    /// stores beside its own document. That id is how the sub-ledger is
    /// reconciled to the general ledger afterwards, and it is the reason this
    /// returns something rather than `()`.
    async fn post(&self, entry: JournalEntry) -> Result<PostedRef, LedgerError>;
}
```

`app-books` implements `Ledger`. `app-inventory` depends on `phonix-ports` and
on nothing of Books at all. Wiring happens once, at the composition root in
`phonix-server`, which is the only place in the workspace that is allowed to
know both apps exist.

### Why a separate crate and not "books exports a trait"

Because then Inventory depends on `app-books`, and a build without Books does
not compile. The whole point of rule 2 is that either app can be left out of a
build. A port crate holds traits and the value types those traits pass, and it
depends on `phonix-core` and nothing else.

### A missing port is an answer, not a crash

An app whose port has no implementation — Books is not compiled in, or not
enabled — gets `None` when it asks for it, and has to have decided in advance
what that means. Inventory's answer:

- Stock movements still happen. Receiving goods is a warehouse fact, and a
  warehouse does not stop because nobody bought the accounting module.
- The journal is not posted, and the movement records that no ledger was
  available rather than recording nothing.

This is what makes rule 2 real rather than aspirational. If "no ledger" were a
failure, Inventory would require Books, and the plugin story would be a
dependency graph wearing a costume.

### The ports this design needs

| Port | Declared for | Implemented by | Used by |
| --- | --- | --- | --- |
| `Ledger` | posting a balanced journal | `app-books` | inventory, sales, payroll |
| `CostCentres` | resolving and validating a cost centre | `app-hr` | books, inventory |
| `Parties` | a party's billing snapshot | `master` | books, inventory |
| `Numbering` | allocating a document number | `core` | every app |
| `Stock` | on-hand and valuation for an item | `app-inventory` | books (COGS on an invoice) |

`Parties` and `Numbering` are ports over things that already exist and are
always present; they are in the table because a port is how an app *should*
reach them, and because writing them down is what stops the sixth app reaching
for the repository directly.

**Wait for the third.** 0001's rule applies to ports harder than to anything
else: a trait extracted for one caller is that caller's service with a `dyn` in
front of it. `CostCentres` is being declared now with two known callers and a
third obvious one, and `Stock` is *not* being declared until Books actually
needs to put cost of goods sold on an invoice.

---

## 3. Codes: the one the system assigns, and the one the world already has

`master.parties.code` is typed by a person, and its migration explains why: "an
accounts department that has called them ACME01 for twenty years is not going to
stop." That is right for a party and wrong for almost everything else.

An item does not arrive with a code. Somebody invents one, and what they invent
is `WIDGET-BLUE-LARGE-2`, then `widget blue lge`, then `WB-L-2`, and eighteen
months later nobody can search the catalogue. Meanwhile the item *does* arrive
with a UPC, which is not ours to invent and must be recorded exactly as printed.

### The rule

> **Every entity gets a generated code. Identifiers the outside world assigned
> are separate, optional, and never generated.**

| | Generated code | External identifier |
| --- | --- | --- |
| Item | `ITM-000431` | UPC, EAN, manufacturer part number |
| Department | `DEPT-004` | — |
| Employee | `EMP-00119` | national insurance number, payroll id |
| Purchase order | `PO-2026-00087` | the supplier's own order reference |
| Invoice | `INV-2026-00042` | the customer's purchase-order number |

The generated code is unique, immutable, and never reused. The external
identifier is whatever the world says it is: possibly absent, possibly
duplicated across two suppliers, possibly wrong, and never a key.

### It uses the machinery that already exists

`core.number_sequences` is the counter, and `config/numbering/<app>.toml` is
where an app declares its formats. That is not a stretch of the mechanism: a
department code and an invoice number are the same problem — a formatted
counter, allocated inside the transaction that writes the row, so a rolled-back
save returns the number.

The one difference is that master-data codes do not need to be gap-free, and
paying for gap-freedom on them means every department created in the workspace
queues through one row. That cost is nothing at the volumes involved — a
workspace creates a few hundred departments in its life and a few hundred
thousand invoices — so the same strict allocator is used for both and there is
one mechanism rather than two. If items ever make that contention real, 0001 §5
already describes the `mode` column that would fix it, and deliberately did not
build it.

### The code is offered, not imposed

The create form shows the code it is going to assign, greyed, with the
allocation not yet made — `Pattern::preview`, never a real counter, for the
reason 0001 §5 rule 3 gives. A workspace migrating from another system needs to
type `ITM-00042` because that is what their labels say, so the field is
editable, and a typed code is validated for uniqueness and left alone. What is
refused is *blank*.

---

## 4. A workspace is never empty

An accounting app whose first screen is "you have no chart of accounts, create
one" has shipped a blank page and called it flexibility. Almost nobody wants to
design a chart of accounts. The people who do want to are the ones who will
change it anyway.

### Two mechanisms, and the difference between them

**Sensible defaults** are rows an app inserts when it is first enabled in a
workspace. They come from `config/defaults/<app_id>.toml`, reviewed in a pull
request, the same discipline as `config/numbering/`. Inserted with
`ON CONFLICT DO NOTHING`, and after that the workspace owns them: a redeploy
never puts back a row somebody deleted, and never overwrites one they edited.

**A setup checklist** is the app declaring what it cannot work without, and the
platform checking it. Not a wizard. A list on the app's home page:

```
Books                                            3 of 5 ready
  ✓  Chart of accounts          312 accounts
  ✓  Financial year             opens April
  ✓  Tax codes                  4 defined
  ✗  Control accounts           receivables account not chosen
  ✗  Opening balances           not entered
```

Every app declares its own items as a `&'static [SetupItem]`, each with a link
to the screen that satisfies it; `phonix_services::workspace::setup` holds the
predicate. The declaration and the predicate are split for the reason
`apps::CATALOG` and `tenancy::apps` are: one compiles to wasm so the browser can
draw the list, and the other holds a pool.
The list is *advisory* for most items and *blocking* for a few: an app whose
blocking items are unsatisfied refuses to post, with a message naming what is
missing rather than a foreign-key violation from four layers down.

Defaults exist so that the checklist is almost always green on the first
morning. Where they cannot be — nobody can guess an opening balance — the
checklist is what stops that being discovered by an accountant in March.

### The chart of accounts is exhaustive

Not a starter set of thirty accounts. A default chart with every account an
ordinary trading company needs, structured, numbered, and typed:

| Range | Class | Normal balance |
| --- | --- | --- |
| 1000–1999 | Assets | debit |
| 2000–2999 | Liabilities | credit |
| 3000–3999 | Equity | credit |
| 4000–4999 | Revenue | credit |
| 5000–5999 | Cost of sales | debit |
| 6000–7999 | Operating expenses | debit |
| 8000–8999 | Other income and expense | — |
| 9000–9999 | Tax and appropriations | — |

Exhaustive is a deliberate choice against the alternative, which is that the
workspace discovers a missing account at the moment it is trying to post
something. An unused account costs a row and a line in a picker that is
searchable anyway; a missing one costs a posting failure and a phone call. The
accounts that matter and that starter charts leave out are precisely the ones a
growing company hits first: goods received not invoiced, inventory adjustment,
purchase price variance, foreign exchange gain and loss, rounding differences,
input and output tax held separately, suspense.

Accounts are typed rather than inferred from their number, because the number is
a convention the workspace may change and the type is what the software reasons
about. `account_type` decides the normal balance, whether the account closes at
year end, and which control accounts a sub-ledger is allowed to name.

---

## 5. What a general ledger has to be

Written down because Books currently has an invoice and no ledger, and the
invoice is the easy half.

1. **Double entry, enforced in the write.** A journal that does not balance is
   not saved and is not saveable. Not validated in a service — refused by the
   type: a `JournalEntry` cannot be constructed unbalanced, so no code path
   reaches the repository with one.
2. **Posting is append-only.** A posted journal is never updated and never
   deleted. A mistake is corrected by a reversing journal that names the one it
   reverses. This is already the invoice's rule and it is the ledger's rule for
   the same reason: a document that can be edited after it was filed is not
   evidence of anything.
3. **Every journal names its source.** `source_app`, `source_doc_type`,
   `source_doc_id`. Reconciling a sub-ledger to the general ledger is then a
   `GROUP BY` rather than an investigation, and "which invoice is this £4,000"
   has an answer that does not involve a human.
4. **Periods lock, and the lock is enforced at post.** A closed period refuses
   a journal dated inside it. Not a warning.
5. **Dimensions are orthogonal to the account.** See §6 — this is the single
   biggest thing the incumbents get wrong.
6. **Currency is the six-column snapshot**, exactly as 0001 §3 specifies, on
   every journal line.

---

## 6. What the incumbents get wrong

The market's mid-tier — QuickBooks, Xero, Sage — is not bad software. It is
software that made a set of choices for the smallest customer and then could not
unmake them. The gaps below are consistent across the comparisons and
integration guides, and each one is a design decision here rather than a feature
to add later.

### 6.1 Inventory valuation is divorced from the ledger

The most expensive gap, and the most common. Reviewers are consistent that none
of the three replaces a real inventory system, that Xero's inventory is basic
and Sage's merely adequate, and that anyone with multiple locations or channels
ends up bolting a separate system on. The bolt-on then owns the quantities while
the accounting package owns the value, and the two disagree by the end of the
first quarter.

The specific failure is that stock movements and journal entries are separate
events that happen to be about the same thing. Nothing structurally forces the
inventory sub-ledger to equal the inventory control account.

**Here:** a stock movement *is* the sub-ledger entry. It carries its own
valuation, and posting it through the `Ledger` port is part of the same
transaction that writes the movement. The control account balance is the sum of
the movements by construction, and a reconciliation screen that shows the two
sides is trivial to write because both sides carry the same `source_doc_id`.

### 6.2 There is no landed cost

Freight, duty, insurance and handling are expensed when the supplier's invoice
arrives, rather than capitalised into the value of the goods. Inventory is
carried below what it cost, every margin on every sale of those goods is
overstated, and the error is invisible because both halves are individually
correct.

**Here:** a landed-cost allocation is a first-class document. It names a receipt
and a set of cost lines, allocates them across the received lines by value,
weight or quantity, and adjusts the valuation layers it touches. The allocation
basis is stored on the document, because "why is this unit 4.12 and that one
4.09" has to be answerable a year later.

### 6.3 Posted transactions stay editable

The one that makes auditors unhappy and is hardest to walk back. In the small
end of this market you can open an invoice from three years ago, change the
amount, and save it — and a period that was reported, filed and audited quietly
restates itself. Nothing is logged in a way that reconstructs what the numbers
were when they were filed.

**Here:** posting is irreversible by construction, corrections are reversing
entries, `core.entity_events` records every change with its before and after,
and period locks refuse a dated write. Three mechanisms, all of which already
exist for other reasons.

### 6.4 Dimensions are an afterthought with a hard limit

Xero gives you exactly two tracking categories. QuickBooks gives you classes and
locations, which do not reach every posting and cannot be made mandatory per
account. Every business that grows past the smallest size wants to slice by
department *and* project *and* location *and* fund, and the ceiling is reached
in year two.

This is why the user's instruction to start with departments and cost centres is
the right instruction, and it is why HR comes before Inventory in §9. A cost
centre is not a reporting nicety bolted onto a journal line; it is a dimension,
and dimensions have to be in the ledger's shape from the first migration because
retrofitting them means rewriting every posting in the system.

**Here:** a journal line carries a set of dimension values, not two named
columns. The dimension *types* are declared — cost centre, project, location,
fund — and an account may require or forbid a given dimension. Cost centres are
supplied by the `CostCentres` port, which is why departments live in HR and are
consumed by Books rather than being a table Books owns.

### 6.5 No three-way match

Purchase orders exist in these systems but do not gate anything. A bill can be
entered and paid for goods that were never received, at a price nobody agreed,
and the only control is that a human noticed.

**Here:** the receipt is the pivot. A bill matches against what was *received*,
not what was ordered, and the tolerances for quantity and price are workspace
settings with a documented default rather than a hardcoded zero. Goods received
and not yet invoiced sit in a GRNI account — one of the accounts a starter chart
leaves out, per §4 — so the liability is on the balance sheet from the moment
the goods arrive rather than from the moment the paperwork does.

### 6.6 Negative stock is permitted

Selling what you do not have produces a negative on-hand quantity, which makes
the average cost a division by a negative number, which makes the valuation
meaningless in a way that persists after the stock is replenished.

**Here:** refused by default, per item and per location, with an explicit
workspace setting to allow it for the operations that genuinely need it — and
where it is allowed, the movement is valued at the last known cost and flagged,
so the correction when the receipt arrives is a known quantity rather than an
archaeology project.

### 6.7 Sub-ledger to general-ledger reconciliation is manual

Consistently the most painful part of the close. Practitioners describe finding
and fixing errors as the hardest part of the work, and the root cause is that
the source data and the financial record were never connected in the first
place — so the connection has to be reconstructed by hand, monthly, forever.

**Here:** §5 rule 3. Every journal names its source document. The connection is
recorded when the posting happens, which is the only moment it is known for
certain.

---

## 7. Inventory is a sub-ledger, and its documents are a chain

The concepts named in the brief are one chain and two things that hang off it.

```
requisition  →  consolidated requisition  →  purchase order  →  receipt  →  bill
                                                                   ↓
                                                            stock movement
                                                                   ↓
                                                          journal (Ledger port)
```

**Requisition.** A department asks for something. It is a request, not a
commitment: no ledger consequence, no supplier, and the cost centre it will be
charged to is on it from the start — which is the `CostCentres` port's first
real caller.

**Consolidated requisition.** Eleven departments each want printer paper. Buying
it eleven times is eleven deliveries at eleven prices. Consolidation groups open
requisition lines by item and turns them into one purchase order, and every line
of that order remembers which requisitions it came from — because when it
arrives, the cost has to be split back across the cost centres that asked for
it, and a consolidation that forgets its inputs cannot do that.

**Purchase order.** The commitment. Numbered, sent, and from this point the
quantities are what everything else is measured against.

**Receipt.** Goods arrive, possibly partially, possibly more than once against
one order. This is the event with the accounting consequence: stock goes up,
GRNI goes up, and the valuation layer is created at the price on the order plus
whatever landed cost is later allocated to it.

**Bill.** The supplier's invoice, matched three ways against the order and the
receipt. Clears GRNI, creates the payable, and any difference between the
ordered price and the billed price is a purchase price variance — posted, not
absorbed silently into the stock value.

**Stock transfer.** Between locations. Two movements, one document, and the
quantity in transit is neither at the origin nor at the destination but is still
on the balance sheet — which is a third state that systems modelling a transfer
as "subtract here, add there" cannot represent, and it is why a transfer is one
document rather than two adjustments.

**Stock adjustment** and **adjustment types.** Every change to stock that is not
a purchase, a sale or a transfer: a count difference, damage, expiry, a sample,
a write-off. The *type* is the point — it decides which account the other side
of the journal goes to, and whether the adjustment needs approval. A workspace
that posts every discrepancy to one "inventory adjustment" account has a number
that grows and tells nobody anything. Types are seeded as defaults per §4, each
naming its account, and the workspace can add their own.

### The stock ledger, as built

The documents above are the interesting half of the brief and the easy half of
the work. Everything they do, they do by moving stock, and five decisions had to
be made to build that underneath them.

**A movement's journal falls out of its two ends.** Each location *kind* stands
for an account role — internal is `Inventory`, transit is `InventoryInTransit`,
a vendor is `GoodsReceivedNotInvoiced`, a customer is `CostOfSales`, inventory
loss is `InventoryAdjustment`. A move then debits the role of where the value
arrived and credits the role of where it left, and the case where those are the
same role is exactly the case where no value moved and no journal is wanted.

That one rule produces the receipt, the delivery, the write-off, the supplier
return, the customer return and the despatch into transit, correctly and without
a document type having an opinion about accounting. It is the reason a return is
not a seventh code path: it is the sixth run backwards.

**A refused journal takes the movement with it.** `NoLedger` is not a refusal —
it is a workspace that never bought the accounting module, and its goods still
arrive; the move is stored with `journal_state = 'no_ledger'` against it. Every
other answer from the port — a closed period, an unmapped role, an account
somebody retired — rolls back the whole transaction. Under automated valuation
there is no state in which a shelf changed and the stock account did not, which
is 6.1 stated as a transaction boundary rather than as an intention.

**Quants are a cache, and the schema can prove it.** What is on hand is stored
per (variant, location, lot) because it is asked on every screen. The *truth* is
`stock_moves`, and the view `stock_quants_reconcile` is the two sides beside
each other: empty is correct, and a row is a bug. This is the practical payoff
of modelling stock as double entry, and without it the cache would be a second
set of books nobody could check.

**Negative stock is refused, and it is not a setting.** A shelf below zero holds
units that were never received, which have no cost, which makes every valuation
after that moment guesswork — 6.6. The floor applies to locations that are ours;
the counterpart locations have none, because a vendor location at −4,000 is a
true statement that four thousand units have been bought.

**A layer per receipt, whatever the costing method.** `remaining` is a
*quantity* and is kept accurate under all three methods, so a workspace can
change costing method without its history already being nonsense. What differs
is how the quantity is *valued*: FIFO uses each layer's own cost, and standard
and average use the item's one number. The average is recomputed on receipt and
never on issue, so the order two pickers happened to work in cannot change what
the month cost.

---

## 8. Where each thing lives

```
crates/
  phonix-core        infrastructure. Money, numbering, permissions, i18n.
  phonix-ports       traits only. Depends on core; knows no app.
  phonix-master      parties, taxes. Always on.
  app-hr             departments and cost centres.  Implements CostCentres.
  app-books          chart of accounts, journals, AR/AP.  Implements Ledger.
  app-inventory      items, stock, procurement.  Implements Stock.
```

with the corresponding schema per app in every tenant database, the migration
stream per app under `migrations/apps/<id>/`, and no foreign key between any two
of them.

Books and Inventory are being designed together, as the brief asks, and that is
the reason `phonix-ports` exists at all: designed apart, Inventory would have
ended up with `app-books` in its `Cargo.toml` on the first day it needed to post
a journal, and rule 2 would have been over before it started.

---

## 9. Human Resources first, and why it is only departments

Starting with HR looks like a detour from an accounting system. It is not, and
§6.4 is the reason: a cost centre is a *dimension*, dimensions have to be in the
ledger's shape from the first migration, and a dimension supplied by another app
through a port is the hardest case. Building it first means the ledger is
designed against a real port with a real implementation, rather than against a
trait somebody will write later.

It is also the smallest possible thing that exercises every rule in this record
at once — one app, one port, one generated code, one set of defaults — which is
what makes it the right first move rather than merely a small one.

### What is built

```sql
CREATE TABLE departments (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code            TEXT NOT NULL,          -- generated: DEPT-###
    name            TEXT NOT NULL,
    parent_id       UUID REFERENCES departments (id) ON DELETE RESTRICT,
    is_cost_centre  BOOLEAN NOT NULL DEFAULT FALSE,
    manager_user_id UUID REFERENCES core.users (id) ON DELETE SET NULL,
    is_active       BOOLEAN NOT NULL DEFAULT TRUE,
    ...
);
```

Four decisions inside that.

**The tree is `parent_id` and nothing cleverer.** A department hierarchy is
shallow — four levels is a large organization — and read whole every time it is
read at all. A closure table or a materialised path buys query performance on a
structure with a few hundred rows in it, and costs a second thing to keep
correct on every move.

**A cost centre is a flag on a department, not a separate table.** They are the
same thing viewed twice: the organizational unit, and the bucket its spending is
charged to. Two tables would mean every department needs a cost centre created
beside it and the two kept in step for ever. The flag is what makes it
selectable in the ledger, and a department without it is a grouping that nothing
posts to — which is exactly what a division above three cost centres should be.

**Not every department is a cost centre and that is the point.** The parent
nodes usually are not. Posting to both a parent and its children is how a
report double-counts.

**Deleting is refused, not cascaded.** `ON DELETE RESTRICT` on the parent, and
the service refuses a department that anything has posted against — which it
cannot ask the database, because no app holds a foreign key into `hr`, so it
asks through the port's inverse: it refuses any department that is a cost centre
and has ever been referenced. Deactivation is offered instead. Same shape as
`master::party::delete`, and for the same reason.

### The port

```rust
// crates/phonix-ports/src/cost_centre.rs
#[async_trait]
pub trait CostCentres {
    /// The cost centres a document may be charged to, for a picker.
    async fn list(&self) -> Result<Vec<CostCentre>, PortError>;

    /// Resolve one, for the moment a posting names it.
    ///
    /// Returns the *snapshot* a document keeps - id, code and name - rather
    /// than a reference, for the reason every other snapshot in this codebase
    /// exists: a department renamed next year must not rewrite last year's
    /// report.
    async fn resolve(&self, id: Uuid) -> Result<Option<CostCentre>, PortError>;
}
```

Books will call `resolve` when a journal line names a cost centre, and store the
code and name it gets back on the line. Inventory will call `list` when a
requisition is raised. Neither depends on `app-hr`.

### What HR is not, yet

No employees, no contracts, no leave, no payroll. Departments are what Books and
Inventory need; the rest is an HR product, and building it now would be building
it before anything asks for it. `hr` as an `app_id` and a schema is chosen with
that growth in mind, rather than calling the app `departments` and having to
rename a schema — which 0001 §2 points out is a primary key in every tenant
database and must never happen.

---

## 10. What this record deliberately does not decide

- **Which costing method.** FIFO, weighted average and standard cost each need a
  different layer discipline, and the choice is per item category rather than
  per workspace. The valuation layer table has to be shaped to hold all three;
  which one an item uses is a later record with the inventory implementation.
- **Approval workflows.** Requisitions want approval, journals sometimes want
  approval, and a general workflow engine is exactly the kind of thing 0001's
  test for `core` refuses — "the moment `core` knows what an approval is, every
  app bends around `core`'s idea of approval." Each app approves its own
  documents until there is a third.
- **The consolidated requisition's grouping rule.** By item is obvious; by
  supplier, by delivery window and by budget period are all defensible and the
  right answer is probably a choice on the consolidation run. Not decided
  without a user in front of it.
- **Multi-entity consolidation.** One workspace is one legal entity here. Group
  reporting across entities is a different product and would change the tenancy
  model, not the app model.

---

## Sources

The gap analysis in §6 draws on:

- [Xero vs QuickBooks vs Sage (UK 2026): Practical Comparison](https://bankreconciler.app/blogXeroVsQuickBooksVsSage)
- [Top 5 QuickBooks Inventory Management Software Integrations in 2026](https://simplydepo.com/industry/quickbooks-inventory-management-software-integrations/)
- [The Key to Using Inventory Cost Accounting Methods in Your Business — NetSuite](https://www.netsuite.com/portal/resource/articles/inventory-management/inventory-cost-accounting-methods-examples.shtml)
- [NetSuite Standard Costing: Setup & Variances (2026)](https://www.brokenrubik.com/blog/netsuite-standard-costing-guide)
- [Navigating the Maze of the Subledger Close Cycle — insightsoftware](https://insightsoftware.com/blog/navigating-the-maze-of-the-subledger-close-cycle/)
- [A Guide for General Ledger to Subledger Reconciliation — Leapfin](https://www.leapfin.com/blog/general-ledger-to-subledger-reconciliation)
- [The financial close cycle explained: from sub-ledger to reporting — Pacera](https://pacera.com/knowledge-hub/blogs/the-financial-close-cycle-explained-from-sub-ledger-to-reporting/)
