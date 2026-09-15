-- inventory 0014: how much of a delivery has been invoiced.
--
-- The mirror of 0006's `billed` on a receipt line. A despatch on the thirtieth
-- and its invoice on the second is a real state, and this is the column that
-- lets it be counted rather than investigated.
--
-- WHY THE TOTAL LIVES HERE AND NOT ON THE INVOICE
--
-- ADR 0006 section 8: no foreign key between two apps. The invoice is Books'
-- and the delivery line is Inventory's, so the invoice carries a bare id and
-- this column is what Inventory knows about it. Nothing writes it yet - the
-- port that lets Books say so is the next change - so every posted delivery
-- reads as uninvoiced, which is exactly what is true today.

ALTER TABLE delivery_lines
    ADD COLUMN invoiced NUMERIC(19, 6) NOT NULL DEFAULT 0;

ALTER TABLE delivery_lines
    ADD CONSTRAINT delivery_lines_invoiced_not_negative CHECK (invoiced >= 0);

COMMENT ON COLUMN delivery_lines.invoiced IS
    'How much of this line has been invoiced, in the stock unit. The sell-side mirror of receipt_lines.billed.';

-- Goods gone and not yet charged for, by delivery and by age. The sell side of
-- `unbilled_receipts`, and at cost rather than at price: this is what the
-- goods-delivered-not-invoiced balance carries.
CREATE VIEW uninvoiced_deliveries AS
SELECT d.id                             AS delivery_id,
       d.number,
       d.despatched_on,
       d.customer_id,
       d.customer_name,
       o.number                         AS order_number,
       sum((l.quantity - l.invoiced) * l.unit_cost) AS uninvoiced,
       (CURRENT_DATE - d.despatched_on) AS age_days
  FROM deliveries d
  JOIN delivery_lines l ON l.delivery_id = d.id
  LEFT JOIN sales_orders o ON o.id = d.order_id
 WHERE d.state = 'done'
   AND l.invoiced < l.quantity
 GROUP BY d.id, d.number, d.despatched_on, d.customer_id, d.customer_name, o.number
HAVING sum((l.quantity - l.invoiced) * l.unit_cost) <> 0;

COMMENT ON VIEW uninvoiced_deliveries IS
    'Goods delivered and not yet invoiced, by delivery and by age. The aged half of the GDNI balance.';
