-- Sample data for med-app-staging: buying, receiving, and what is on the shelf.
--
-- HOW TO RUN IT
--
--   psql "<med-app-staging connection string>" -f tools/sample-data/inventory-procurement.sql
--
-- Against the TENANT database, not the catalog. Every statement is idempotent:
-- run it twice and nothing doubles.
--
-- WHAT IT DOES NOT DO, AND WHY THAT IS DELIBERATE
--
-- It does not write a single row into `stock_moves`, `stock_quants`,
-- `valuation_layers` or `books.journals`. Those are the ledger, and the ledger
-- is not something a seed script may fabricate:
--
--   * a quant written by hand disagrees with the movement history, which is
--     precisely the bug `stock_quants_reconcile` exists to catch - seeding one
--     would be planting the bug the design is built to detect;
--   * a movement written by hand posts no journal, so the stock account would
--     not agree with the stock report, which is the whole of ADR 0006 section
--     6.1;
--   * the running average, the FIFO layers and the goods-received-not-invoiced
--     entry are all worked out by `stock::apply`, and a script that guessed
--     them would be a second implementation of the costing rules.
--
-- So this seeds everything UP TO the ledger and leaves two clicks:
--
--   1. Open Inventory > Purchase orders, open PO-SAMPLE-1 and Confirm it.
--   2. Open Inventory > Goods receipts, open the draft and Post it.
--
-- That posts the journal, moves the quants, opens the valuation layers and
-- advances the order's received quantities through the real code path - which
-- is the only way the sample data is *correct* rather than merely present.
-- After those two clicks, Stock on hand and Movements both have something in
-- them, and `SELECT * FROM inventory.stock_quants_reconcile` is still empty.
--
-- WHAT IT SEEDS
--
-- A medical wholesaler's shelf: two lot-tracked consumables with expiry dates,
-- one serial-tracked instrument, and one untracked item, so FEFO, recall and
-- the plain case are all represented. Prices are in the workspace's own
-- currency; change `SAMPLE_CURRENCY` below if that is not GBP.

BEGIN;

SET LOCAL search_path = inventory, public;

-- ---------------------------------------------------------------------------
-- What this script needs to already be there
-- ---------------------------------------------------------------------------
--
-- Checked rather than assumed, because the failure this stops is silent: every
-- insert below is a SELECT with a join, and a join that matches nothing inserts
-- nothing and reports success. A workspace missing its `Each` unit seeded four
-- items, zero rows, and no error.

DO $precondition$
DECLARE
    missing text;
BEGIN
    SELECT string_agg(what, '; ') INTO missing FROM (
        SELECT 'no default warehouse' AS what
         WHERE NOT EXISTS (SELECT 1 FROM warehouses WHERE is_default)
        UNION ALL
        SELECT 'no unit in the count class'
         WHERE NOT EXISTS (SELECT 1 FROM units WHERE class = 'count' AND is_active)
        UNION ALL
        SELECT 'the root category All is missing'
         WHERE NOT EXISTS (SELECT 1 FROM categories WHERE path = 'All')
        UNION ALL
        SELECT 'the organization has no currency'
         WHERE NOT EXISTS (
             SELECT 1 FROM core.organization_profile WHERE currency_code <> ''
         )
    ) t;

    IF missing IS NOT NULL THEN
        RAISE EXCEPTION 'this workspace is not ready for the sample data: %', missing
              USING HINT = 'let the server finish its migrations, then run this again';
    END IF;
END
$precondition$;

-- ---------------------------------------------------------------------------
-- The supplier
-- ---------------------------------------------------------------------------

INSERT INTO master.parties (code, kind, name, legal_name, email, is_active)
VALUES ('MEDSUP01', 'organization', 'Meridian Medical Supplies',
        'Meridian Medical Supplies Ltd', 'orders@example.invalid', TRUE)
ON CONFLICT DO NOTHING;

-- The role is what makes them offerable on a purchase order. Without it the
-- order form refuses them, which is the check working rather than a gap.
INSERT INTO master.party_roles (party_id, role)
SELECT id, 'supplier' FROM master.parties WHERE code = 'MEDSUP01'
ON CONFLICT DO NOTHING;

-- ---------------------------------------------------------------------------
-- A category, so the costing method is visible rather than inherited silently
-- ---------------------------------------------------------------------------
--
-- FIFO with automated valuation: the combination an auditor asks for, and the
-- one that exercises the valuation layers rather than the single-cost path.

INSERT INTO categories (path, name, parent_id, costing_method, valuation, removal_strategy)
SELECT 'All/Medical consumables', 'Medical consumables', root.id, 'fifo', 'automated', 'fefo'
  FROM categories root
 WHERE root.path = 'All'
ON CONFLICT (path) DO NOTHING;

-- ---------------------------------------------------------------------------
-- The items
-- ---------------------------------------------------------------------------
--
-- Codes are typed here rather than allocated, because a seed script has no
-- business drawing from the workspace's number series - a gap in `ITM-#####`
-- caused by sample data is a gap somebody would have to explain. `SAMPLE-`
-- prefixed codes are also what makes this script removable.

INSERT INTO items (code, name, description, kind, is_tracked, tracking, uses_expiry,
                   category_id, stock_unit_id, purchase_unit_id, cost, sale_price,
                   can_be_purchased, can_be_sold)
SELECT v.code, v.name, v.description, 'goods', TRUE, v.tracking, v.uses_expiry,
       c.id, u.id, u.id, v.cost, v.sale_price, TRUE, TRUE
  FROM (VALUES
        ('SAMPLE-GLOVE-M',
         'Nitrile examination gloves, medium',
         'Box of 100. Lot-tracked with an expiry, which is what makes a recall a query.',
         'lot', TRUE, 4.2000, 7.5000),
        ('SAMPLE-SYRINGE-5',
         'Disposable syringe 5 ml',
         'Box of 50. Lot-tracked with an expiry.',
         'lot', TRUE, 6.8000, 11.0000),
        ('SAMPLE-SCOPE-01',
         'Diagnostic otoscope',
         'Serial-tracked: one number covers one unit, and a warranty claim needs it.',
         'serial', FALSE, 148.0000, 240.0000),
        ('SAMPLE-WIPE-70',
         'Alcohol wipes, 70 percent',
         'Untracked. The plain case, so the screens are not all lot numbers.',
         'none', FALSE, 2.1000, 3.9500)
       ) AS v(code, name, description, tracking, uses_expiry, cost, sale_price)
  CROSS JOIN LATERAL (
      SELECT id FROM categories WHERE path = 'All/Medical consumables'
  ) c
  CROSS JOIN LATERAL (
      -- `EA` where it exists, and otherwise whatever this workspace counts in.
      -- Not by id, because ids are per database, and not by code alone: a
      -- workspace whose `Each` was deleted or renamed still counts things, and
      -- a sample script is not the place to discover that.
      SELECT id FROM units
       WHERE class = 'count' AND is_active
       ORDER BY (code = 'EA') DESC, is_base DESC, factor
       LIMIT 1
  ) u
 WHERE NOT EXISTS (SELECT 1 FROM items i WHERE lower(i.code) = lower(v.code));

-- Every item has exactly one default variant, and stock hangs off the variant.
-- 0002 does this for items that predate it; a seed script has to do it for the
-- rows it creates itself.
INSERT INTO item_variants (item_id, code, is_default)
SELECT i.id, i.code, TRUE
  FROM items i
 WHERE i.code LIKE 'SAMPLE-%'
   AND NOT EXISTS (SELECT 1 FROM item_variants v WHERE v.item_id = i.id);

-- ---------------------------------------------------------------------------
-- A purchase order, left as a draft
-- ---------------------------------------------------------------------------
--
-- No number: a draft has none, and confirming it is what allocates one from
-- the real series. That is the rule the schema enforces with
-- `purchase_orders_numbered_when_confirmed`, and seeding a number here would
-- need a state this row is not in.

INSERT INTO purchase_orders (state, supplier_id, supplier_code, supplier_name,
                             warehouse_id, order_date, expected_on, currency, net,
                             supplier_reference, note)
SELECT 'draft', p.id, p.code, p.name, w.id,
       CURRENT_DATE - 3, CURRENT_DATE + 4, org.currency_code, 0,
       'MMS-Q-88421',
       'Sample data. Confirm this order, then post the draft receipt against it.'
  FROM master.parties p
  CROSS JOIN LATERAL (SELECT id FROM warehouses WHERE is_default LIMIT 1) w
  CROSS JOIN LATERAL (
      SELECT currency_code FROM core.organization_profile LIMIT 1
  ) org
 WHERE p.code = 'MEDSUP01'
   AND NOT EXISTS (
       SELECT 1 FROM purchase_orders o WHERE o.supplier_reference = 'MMS-Q-88421'
   );

INSERT INTO purchase_order_lines (order_id, line_no, variant_id, description,
                                  quantity, unit_id, quantity_stock, unit_price, net)
SELECT o.id, v.line_no, var.id, i.name,
       v.quantity, i.stock_unit_id, v.quantity, v.unit_price,
       round(v.quantity * v.unit_price, 4)
  FROM (VALUES
        (1, 'SAMPLE-GLOVE-M',  40.000000, 4.2000),
        (2, 'SAMPLE-SYRINGE-5', 25.000000, 6.8000),
        (3, 'SAMPLE-WIPE-70',  120.000000, 2.1000)
       ) AS v(line_no, code, quantity, unit_price)
  JOIN items i ON lower(i.code) = lower(v.code)
  JOIN item_variants var ON var.item_id = i.id AND var.is_default
  CROSS JOIN LATERAL (
      SELECT id FROM purchase_orders WHERE supplier_reference = 'MMS-Q-88421' LIMIT 1
  ) o
 WHERE NOT EXISTS (
     SELECT 1 FROM purchase_order_lines l WHERE l.order_id = o.id AND l.line_no = v.line_no
 );

-- The order's total, from its own lines rather than typed twice.
UPDATE purchase_orders o
   SET net = COALESCE((
           SELECT sum(l.net) FROM purchase_order_lines l WHERE l.order_id = o.id
       ), 0)
 WHERE o.supplier_reference = 'MMS-Q-88421';

-- ---------------------------------------------------------------------------
-- A draft receipt, short on one line
-- ---------------------------------------------------------------------------
--
-- Deliberately short: 40 gloves ordered and 30 arrived. Posting it leaves the
-- order partly received with 10 outstanding, which is what a backorder is and
-- the case worth having in sample data - a fully-received order demonstrates
-- nothing about the interesting half of the screen.
--
-- Lot numbers and expiry dates are typed here as they would be off the carton.
-- The `lots` rows themselves are created when the receipt is posted, which is
-- what `lot::ensure` is for.

INSERT INTO receipts (state, order_id, supplier_id, supplier_code, supplier_name,
                      warehouse_id, to_location_id, received_on, delivery_note, note)
SELECT 'draft', o.id, p.id, p.code, p.name, w.id, w.receiving_location_id,
       CURRENT_DATE, 'MMS-DN-55130',
       'Sample data. Post this to move the stock and file the journal.'
  FROM master.parties p
  CROSS JOIN LATERAL (
      -- The same choice `receiving_location` makes: one-step warehouses receive
      -- straight into stock, and the others into Input.
      SELECT wh.id,
             CASE
                 WHEN wh.receipt_steps = 'one_step' THEN wh.stock_location_id
                 ELSE COALESCE(
                     (SELECT loc.id FROM locations loc
                       WHERE loc.warehouse_id = wh.id AND loc.kind = 'internal'
                         AND loc.name = 'Input' AND loc.is_active
                       ORDER BY loc.path LIMIT 1),
                     wh.stock_location_id
                 )
             END AS receiving_location_id
        FROM warehouses wh
       WHERE wh.is_default
       LIMIT 1
  ) w
  CROSS JOIN LATERAL (
      SELECT id FROM purchase_orders WHERE supplier_reference = 'MMS-Q-88421' LIMIT 1
  ) o
 WHERE p.code = 'MEDSUP01'
   AND NOT EXISTS (
       SELECT 1 FROM receipts r WHERE r.delivery_note = 'MMS-DN-55130'
   );

INSERT INTO receipt_lines (receipt_id, line_no, order_line_id, variant_id, description,
                           quantity, lot_number, expires_on, unit_cost, value)
SELECT r.id, v.line_no, ol.id, var.id, i.name,
       v.quantity, v.lot_number, v.expires_on, v.unit_cost,
       round(v.quantity * v.unit_cost, 4)
  FROM (VALUES
        (1, 'SAMPLE-GLOVE-M',   30.000000, 'MMS-2026-0917', (CURRENT_DATE + 540)::date, 4.2000),
        (2, 'SAMPLE-SYRINGE-5', 25.000000, 'MMS-2026-0918', (CURRENT_DATE + 730)::date, 6.8000),
        (3, 'SAMPLE-WIPE-70',  120.000000, NULL,            NULL,                        2.1000)
       ) AS v(line_no, code, quantity, lot_number, expires_on, unit_cost)
  JOIN items i ON lower(i.code) = lower(v.code)
  JOIN item_variants var ON var.item_id = i.id AND var.is_default
  CROSS JOIN LATERAL (
      SELECT id FROM receipts WHERE delivery_note = 'MMS-DN-55130' LIMIT 1
  ) r
  LEFT JOIN LATERAL (
      SELECT l.id
        FROM purchase_order_lines l
        JOIN purchase_orders o ON o.id = l.order_id
       WHERE o.supplier_reference = 'MMS-Q-88421' AND l.variant_id = var.id
       LIMIT 1
  ) ol ON TRUE
 WHERE NOT EXISTS (
     SELECT 1 FROM receipt_lines rl WHERE rl.receipt_id = r.id AND rl.line_no = v.line_no
 );

COMMIT;

-- ---------------------------------------------------------------------------
-- What to check afterwards
-- ---------------------------------------------------------------------------
--
-- After confirming the order and posting the receipt, all four of these should
-- hold. The first is the one that matters: it is the schema proving its own
-- cache, and it is the reason none of the above wrote a quant by hand.
--
--   -- Empty. A row here is a quant that disagrees with the movements.
--   SELECT * FROM inventory.stock_quants_reconcile;
--
--   -- 30 gloves, 25 syringes, 120 wipes, at the warehouse's stock location.
--   SELECT * FROM inventory.stock_quants;
--
--   -- Three movements, each naming the journal it posted.
--   SELECT moved_on, quantity, value, journal_state, journal_number
--     FROM inventory.stock_moves ORDER BY created_at;
--
--   -- Ten gloves still owed: the backorder.
--   SELECT l.description, l.quantity_stock, l.received,
--          l.quantity_stock - l.received AS outstanding
--     FROM inventory.purchase_order_lines l
--     JOIN inventory.purchase_orders o ON o.id = l.order_id
--    WHERE o.supplier_reference = 'MMS-Q-88421';
--
-- And the accounting side, which is the point of all of it: inventory debited
-- and goods-received-not-invoiced credited, by the same amount, once per line.
--
--   SELECT j.number, j.narration, a.number AS account, jl.side, jl.amount
--     FROM books.journal_lines jl
--     JOIN books.journals j ON j.id = jl.journal_id
--     JOIN books.accounts a ON a.id = jl.account_id
--    WHERE j.source_app = 'inventory'
--    ORDER BY j.created_at, jl.side;
