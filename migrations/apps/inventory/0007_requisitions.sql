-- inventory 0007: the request that comes before the commitment.
--
-- WHAT THIS ADDS
--
--   requisitions        a department asking for something
--   requisition_lines   what they want, how many, and how much has been ordered
--
-- WHY A REQUISITION IS NOT A DRAFT PURCHASE ORDER
--
-- They look alike and they are answerable by different people. A purchase order
-- is a promise to a supplier, made by whoever may commit the workspace's money.
-- A requisition is a request from somebody who may not: it names no supplier, no
-- price and no currency, because the person raising it does not know any of the
-- three and should not be asked to invent them. ADR 0006 section 7.
--
-- The consequence worth stating: nothing here ever reaches the ledger. There is
-- no `journal_id`, no `journal_state` and no valuation column on either table,
-- and that is not an omission - a request that moved an account would be a
-- commitment with a different name.
--
-- THE COST CENTRE IS ON IT FROM THE START
--
-- Not added at approval, and not inferred from whoever happens to receive the
-- goods. "Who is paying for this" is the first question a requisition answers,
-- and a workspace that leaves it until the invoice arrives is one where the
-- answer is whoever shouts least. This is the `CostCentres` port's first caller
-- from a document, and it is carried the way every port result is - a bare id
-- with the code and name snapshotted beside it, no foreign key, because
-- Inventory may not name `hr.departments`. ADR 0006 section 2.
--
-- `cost_centre_id` is NOT NULL, and that is a deliberate departure from the
-- spirit of ADR 0006 section 2, taken knowingly: it means a workspace without
-- the HR app cannot raise a requisition at all, because there is no department
-- to charge it to. An optional column was built first and overruled, on the
-- grounds that a requisition whose payer is unknown is the very document this
-- one exists to replace. The port is still how the id is resolved; what changed
-- is that an empty answer is now a refusal rather than a blank.
--
-- THE NUMBER IS ALLOCATED AT SUBMIT
--
-- Same rule as the order's at confirm and the receipt's at post: a draft
-- somebody abandoned must not leave a hole in a series. Until it is submitted a
-- requisition is somebody's shopping list and has no number.
--
-- EVERY LINE NAMES AN ITEM
--
-- `variant_id` and `unit_id` are both NOT NULL. A line that merely described
-- something the workspace does not stock was allowed first and overruled: a
-- request naming no item cannot be grouped, cannot be priced, and cannot become
-- an order line without somebody retyping it. The workflow cost is real and
-- accepted - "I need a thing we do not stock yet" is now two steps, the item
-- first and the request second.
--
-- The payoff is that consolidation has no second case: every outstanding line is
-- groupable by (variant, warehouse), which `requisition_demand` below can now
-- assume rather than filter for.
--
-- WHAT `ordered` IS FOR
--
-- A quantity, not a flag, on the same terms as `purchase_order_lines.received`.
-- One requisition line can be satisfied by two orders - half now and half when
-- the supplier has the rest - so "has it been ordered" is arithmetic over the
-- line rather than a box somebody ticks. 0008 will add the link table that says
-- WHICH order lines advanced it; this column is what they advance.
--
-- No triggers, per ADR 0006 and the note at the head of 0005. The CHECK
-- constraints below are row-local facts, which is what a constraint is for.

-- ---------------------------------------------------------------------------
-- Requisitions
-- ---------------------------------------------------------------------------

CREATE TABLE requisitions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- `REQ-2026-00042`. Empty until submitted; see the header.
    number      TEXT NOT NULL DEFAULT '',

    -- draft | submitted | approved | rejected | cancelled
    --
    -- No `ordered` and no `partially_ordered`. How much has been ordered is
    -- arithmetic over the lines, and storing it as a state is a second fact
    -- about the same thing that stops agreeing the first time an order is
    -- cancelled. See `app_inventory::requisition::Requisition::order_state`,
    -- which is `PurchaseOrder::receipt_state` applied to the line above it.
    state       TEXT NOT NULL DEFAULT 'draft',

    -- An `hr.departments` id with NO foreign key, resolved through the
    -- `CostCentres` port and snapshotted beside. Required - see the header.
    cost_centre_id   UUID NOT NULL,
    cost_centre_code TEXT NOT NULL,
    cost_centre_name TEXT NOT NULL,

    -- Where the goods are wanted. A requisition is raised against a place
    -- somebody works, and an order raised from it inherits this.
    warehouse_id UUID NOT NULL REFERENCES warehouses (id) ON DELETE RESTRICT,

    raised_on   DATE NOT NULL,
    -- When they need it by. NULL is ordinary: "when you can" is a real answer,
    -- and a date invented to fill the column would be a deadline nobody set.
    needed_by   DATE,

    -- Why they want it, in their words. The field an approver actually reads,
    -- which is why it is longer than a note and not called one.
    justification TEXT,
    note        TEXT,

    -- Who decided, and what they said. `decided_by` is NOT the approver's
    -- department: a decision is made by a person.
    --
    -- `decision_note` is required on BOTH answers. An approval that explains
    -- itself is what somebody reads a year later when the spend is queried, and
    -- an approval nobody had to justify is the one that gets given without being
    -- read.
    decided_at  TIMESTAMPTZ,
    decided_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    decision_note TEXT,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID REFERENCES core.users (id) ON DELETE SET NULL,
    submitted_at TIMESTAMPTZ,
    submitted_by UUID REFERENCES core.users (id) ON DELETE SET NULL,

    CONSTRAINT requisitions_state_known CHECK (
        state IN ('draft', 'submitted', 'approved', 'rejected', 'cancelled')
    ),
    -- A number and a submission are one fact. `rejected` and `cancelled` keep
    -- theirs: a requisition that was turned down is one somebody has to be able
    -- to look up by the number they were quoted.
    CONSTRAINT requisitions_numbered_when_submitted CHECK (
        (state = 'draft') = (number = '')
    ),
    -- The snapshot beside the id is the port's own answer, so it is present and
    -- non-empty or the row is wrong.
    CONSTRAINT requisitions_cost_centre_named CHECK (
        char_length(cost_centre_code) BETWEEN 1 AND 40
        AND char_length(cost_centre_name) BETWEEN 1 AND 200
    ),
    -- A decision is a time, a decider and a reason together, and only the two
    -- states that are decisions carry one. Both halves in one constraint,
    -- because "decided" is a single fact and splitting it lets half of it be
    -- true.
    CONSTRAINT requisitions_decided_when_settled CHECK (
        (state IN ('approved', 'rejected'))
        = (decided_at IS NOT NULL AND decision_note IS NOT NULL)
    ),
    CONSTRAINT requisitions_needed_after_raised CHECK (
        needed_by IS NULL OR needed_by >= raised_on
    ),
    CONSTRAINT requisitions_justification_length CHECK (
        justification IS NULL OR char_length(justification) <= 2000
    ),
    CONSTRAINT requisitions_note_length CHECK (
        note IS NULL OR char_length(note) <= 2000
    ),
    CONSTRAINT requisitions_decision_note_length CHECK (
        decision_note IS NULL OR char_length(decision_note) BETWEEN 1 AND 2000
    )
);

-- Partial, so the many drafts with no number do not collide with each other.
CREATE UNIQUE INDEX requisitions_number ON requisitions (number) WHERE number <> '';
CREATE INDEX requisitions_raised ON requisitions (raised_on DESC);
CREATE INDEX requisitions_warehouse ON requisitions (warehouse_id);
-- "What is waiting on me" is the query an approver opens with, so the state
-- leads. "What has this department asked for" is the other one worth an index.
CREATE INDEX requisitions_awaiting ON requisitions (state, raised_on)
    WHERE state = 'submitted';
CREATE INDEX requisitions_cost_centre ON requisitions (cost_centre_id, raised_on DESC)
    WHERE cost_centre_id IS NOT NULL;

COMMENT ON TABLE requisitions IS
    'A department asking for something. A request and not a commitment: no supplier, no price, and nothing that reaches the ledger. Requires a cost centre, and therefore requires the HR app.';

CREATE TABLE requisition_lines (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    requisition_id UUID NOT NULL REFERENCES requisitions (id) ON DELETE CASCADE,

    -- Position on the printed request, from one.
    line_no     INTEGER NOT NULL,

    -- What they want. Required - see the header. The description beside it is
    -- still the requester's own words, because "the 5ml ones, not the 10ml" is
    -- worth carrying even when the variant already says which.
    variant_id  UUID NOT NULL REFERENCES item_variants (id) ON DELETE RESTRICT,
    description TEXT NOT NULL,

    quantity    NUMERIC(19, 6) NOT NULL,
    -- The unit they are asking in.
    unit_id     UUID NOT NULL REFERENCES units (id) ON DELETE RESTRICT,

    -- How much of this line has been put on a purchase order. Advanced by
    -- 0008's consolidation, and by raising an order from one requisition.
    -- A quantity rather than a flag - see the header.
    ordered     NUMERIC(19, 6) NOT NULL DEFAULT 0,

    -- What the requester thinks it costs, in the workspace's own currency.
    -- Advisory and never posted: it is what makes a total an approver can weigh
    -- a decision against, and it is explicitly NOT what the order is priced at.
    estimate    NUMERIC(19, 4),

    note        TEXT,

    CONSTRAINT requisition_lines_quantity_positive CHECK (quantity > 0),
    CONSTRAINT requisition_lines_ordered_not_negative CHECK (ordered >= 0),
    -- Ordering more than was asked for is a purchasing decision and belongs on
    -- the order, not smuggled back onto somebody else's request.
    CONSTRAINT requisition_lines_ordered_within_request CHECK (ordered <= quantity),
    CONSTRAINT requisition_lines_description_length CHECK (
        char_length(description) BETWEEN 1 AND 400
    ),
    CONSTRAINT requisition_lines_estimate_not_negative CHECK (
        estimate IS NULL OR estimate >= 0
    ),
    CONSTRAINT requisition_lines_note_length CHECK (
        note IS NULL OR char_length(note) <= 2000
    )
);

CREATE UNIQUE INDEX requisition_lines_position
    ON requisition_lines (requisition_id, line_no);
CREATE INDEX requisition_lines_variant ON requisition_lines (variant_id);
-- What consolidation reads: every line with something still to order, grouped by
-- what it is. The partial index is the query.
CREATE INDEX requisition_lines_outstanding ON requisition_lines (variant_id)
    WHERE ordered < quantity;

COMMENT ON COLUMN requisition_lines.estimate IS
    'What the requester thinks one unit costs. Advisory, never posted, and not what the order is priced at.';

-- ---------------------------------------------------------------------------
-- What is still to be ordered
-- ---------------------------------------------------------------------------
--
-- The other half of "what have we been asked for". A view rather than a
-- materialised total for the reason `stock_quants_reconcile` is a view: the
-- lines are the truth, and a second copy of this figure is a second thing to
-- keep right.
--
-- Approved only. A submitted requisition is a question nobody has answered yet,
-- and consolidating it would be buying on the strength of a request that could
-- still be turned down.

CREATE VIEW requisition_demand AS
SELECT l.variant_id,
       r.warehouse_id,
       count(*)                        AS lines,
       count(DISTINCT r.id)            AS requisitions,
       sum(l.quantity - l.ordered)     AS outstanding,
       min(r.needed_by)                AS needed_by,
       min(r.raised_on)                AS oldest_request
  FROM requisition_lines l
  JOIN requisitions r ON r.id = l.requisition_id
 WHERE r.state = 'approved'
   AND l.ordered < l.quantity
 GROUP BY l.variant_id, r.warehouse_id;

COMMENT ON VIEW requisition_demand IS
    'Approved requisition lines with something still to order, grouped by what and where. What consolidation turns into one purchase order.';
