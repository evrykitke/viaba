-- books 0009: which delivery an invoice line is charging for.
--
-- The last of the three pieces that connect goods going out to money being
-- asked for. Inventory counts what it has had invoiced against it and offers
-- the `Deliveries` port; this is the id the port is given.
--
-- NO FOREIGN KEY, AND THAT IS THE POINT
--
-- `inventory.delivery_lines` belongs to another app with its own migration
-- stream, and ADR 0006 section 8 forbids a key between two of them. Held the
-- same way `tax_group_id` above is held and for a weaker reason than this one:
-- a tax group is merely elsewhere, a delivery line is somebody else's.
--
-- Nothing here enforces that the quantity billed is one that was delivered.
-- That is the port's job, because it is the only place that can lock the
-- delivery line and the only place that knows what is left on it.

ALTER TABLE invoice_lines
    ADD COLUMN delivery_line_id UUID;

COMMENT ON COLUMN invoice_lines.delivery_line_id IS
    'The inventory.delivery_lines row this line bills. A bare id across an app boundary - resolved through the Deliveries port, never joined.';

-- What a repair would read: the invoiced total Inventory caches on a delivery
-- line is the sum of the posted invoice lines naming it.
CREATE INDEX invoice_lines_delivery_line ON invoice_lines (delivery_line_id)
    WHERE delivery_line_id IS NOT NULL;
