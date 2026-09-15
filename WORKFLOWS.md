# Workflow verification list

Every workflow the reference products define, against what viaba actually has.
This is the checklist the sales and people revamps are measured by — and the
place to record that something is **deliberately** absent, which is different
from missing.

Sources:

- ERPNext Selling — <https://docs.frappe.io/erpnext/selling>
- Odoo 19 Sales — <https://www.odoo.com/documentation/19.0/applications/sales/>
- Frappe HR — <https://docs.frappe.io/hr/introduction>

Status marks:

| | |
| --- | --- |
| `[x]` | built, and verified in the code on 2026-09-15 |
| `[ ]` | absent, and verified absent |
| `[~]` | partly built — the note says which half |
| `[?]` | **not verified** — nobody has checked, do not trust either way |
| `[-]` | deliberately not built; the note says where the reasoning lives |

---

## Selling: the order-to-cash chain

- [x] **Quotation** — offer to a customer, revisable, expires.
      viaba models this the Odoo way, not the ERPNext way: `SaleState::Sent` on
      the sales order record, with `valid_until` and expiry. *"A quotation and
      an order are one document in two states."*
      `app-inventory/src/sales_order.rs`
- [x] **Sales order** — confirmed quantities, prices and terms. Carries two
      running totals, shipped and billed. Same record as the quotation.
      `app-inventory/src/sales_order.rs`
- [x] **Delivery** — stock leaves, the balance sheet loses it, cost lands in P&L.
      One document over N stock moves, each its own transaction.
      `app-inventory/src/delivery.rs`
- [x] **Sales invoice** — posts receivable, revenue and output tax through
      account determination. Voiding reverses it.
      `app-books/src/invoice.rs`
- [x] **Payment** — lands in a bank or cash account, allocated across invoices
      as a relation rather than a `paid` column. Overpayment sits on account.
      `app-books/src/payment.rs`
- [x] **Credit note / sales return** — a credit note is an invoice with
      `kind = credit_note`: its own numbering series, and a journal that is the
      invoice entry backwards with the amounts still positive, raised from the
      invoice it credits. The statement, the ageing, money on account, the
      front page, both settlement guards and the invoice itself all net one
      off.
- [x] **Invoice billing a delivery** — an invoice line may carry a
      `delivery_line_id`, and posting raises the `Deliveries` port, which
      refuses more than was delivered. Inventory's `uninvoiced_deliveries` is
      the aged accrual. Raised from the despatch: the delivery offers it and
      the invoice opens prefilled with what is left to charge for.

## Selling: pricing

- [x] **Price determination** — `app-inventory/src/price_list.rs` resolves
      which price applies; `app-books/src/pricing.rs` turns it into a line
      amount and never asks where it came from.
- [x] **Price list** — ERPNext's Price List + Item Price, Odoo's pricelists.
      Named lists in one currency, prices per variant with quantity breaks and
      validity windows, one list per customer, and the rule that picks between
      overlapping rows. A sales order line opens on what that customer is
      quoted and re-prices when the quantity crosses a break. Deliberately
      absent: no fallback to `items.sale_price` — see ADR 0006 section 7.
- [ ] **Pricing rule / promotional scheme** — conditional discounts, margins,
      slabs. Depends on price lists existing first.
- [?] **Discounts** — `discount` appears in `pricing.rs`, `invoice.rs` and
      `account.rs`. Nobody has checked how far it goes.

## Selling: invoicing policy

- [?] **Ordered vs delivered quantity** — Odoo's invoicing policy. The sales
      order carries both a shipped and a billed total, so the data is there;
      whether a policy chooses between them is unverified.
- [ ] **Down payments** — Odoo raises a down-payment invoice against an order.
- [?] **Payment terms** — unverified on the customer record.

## Selling: commercial structure

- [ ] **Sales person** — assigned to a transaction, performance tracked.
- [ ] **Sales partner / commission** — ERPNext Sales Partner, Odoo Commissions.
- [ ] **Territory** — customer segmentation for reporting and policy.
- [ ] **Blanket order** — a standing agreement drawn down by orders.
- [ ] **Product bundle** — one sellable line over several stock items.
- [ ] **Loyalty programme.**
- [-] **Marketplace connectors** (Amazon, Shopee, Lazada, TikTok, Gelato) —
      Odoo ships these; out of scope for viaba unless you say otherwise.

## People: Frappe HR's six areas

`app-hr` is four files today — `employee.rs`, `department.rs`,
`job_position.rs`, `work_location.rs` — over five tables: `employees`,
`engagements`, `assignments`, `job_positions`, `work_locations`.

**The rule everything below must obey:** what changes about somebody is a dated
`assignments` row, never a column on `employees`. Employment is an
`engagements` row; a rehire is a second engagement. Any workflow here that
wants a status column on a person is the bug this schema was built to refuse.

- [x] **Employee record** — name, contact, identifier. Only what survives a
      promotion.
- [x] **Org structure** — departments as a tree, job positions as rows so a
      vacancy exists, work locations.
- [x] **Assignments** — department, role, manager, place, all dated.
- [x] **Engagements** — employment as a dated row with a reason on the outcome.
- [~] **Employee lifecycle** — promotion, transfer and exit are documents
      that write the dated rows on confirm, with a screen and a personnel
      file on the employee record. Onboarding is deliberately not one:
      hiring already has a document. No exit interview.
- [-] **Leave** — not built, deliberately. See ADR 0006 §9.
- [x] **Holiday calendar** — regional holiday lists with a span, days off, and
      a weekly-off generator. Which calendar applies is a dated assignment,
      so a date is asked against the one in force then.
- [x] **Attendance** — check-in/check-out, with each day resolved against the
      holiday calendar in force then. Geolocation is deliberately **not**
      built — decided 2026-09-15.
- [x] **Shift management** — shift types with hours and grace windows, the
      shift as a dated assignment, and lateness on the timesheet, resolved
      in the workspace's own time zone. Rosters beyond the dated assignment
      are not built.
- [-] **Payroll and taxation** — salary structures, tax slabs, salary slips.
      Not built, deliberately. See ADR 0006 §9.
- [-] **Contracts / salary on the employee** — not built, deliberately.
      See ADR 0006 §9.
- [ ] **Expense claims and advances** — multi-level approval, posts to the
      ledger. Needs `app-books`.
- [ ] **Performance management** — goals, key result areas, appraisal cycles.
- [x] **Recruitment** — job applicants against the existing job-position rows,
      with named stages, a vacancy that counts who is waiting, and a hire
      that opens a second engagement on an existing record rather than
      duplicating the person. No job postings and no offers as documents.
- [ ] **Timesheets.**
- [-] **Mobile application** — Frappe HR ships one. Out of scope.

---

## Before building anything marked `[-]`

Those are recorded decisions, not gaps. ADR 0006 §9 explains each, and the
reasoning is usually that the obvious simplification is the exact bug the design
avoids. Read it, and if the decision should change, change the ADR in the same
commit — do not quietly build past it.

## Before trusting anything marked `[?]`

Nobody has checked. Verify before you either build it or claim it exists.
