-- inventory 0016: which list a customer is quoted from.
--
-- 0015 built the lists and the rule that picks a price out of one. This is what
-- says which list applies, which is the half that makes a wholesale customer
-- and a walk-in different.
--
-- WHY IT IS A TABLE HERE AND NOT A COLUMN ON A PARTY
--
-- A price list is Inventory's and a party is master's, and ADR 0006 section 8
-- forbids a foreign key between two of them. Putting the list on the party
-- would also make master know what a price list is, which is the wrong way
-- round: master holds who somebody is, and what we charge them is a selling
-- fact. So the row lives here, holding a bare party id - the direction
-- `sales_orders.customer_id` already goes.
--
-- One list per customer, so the primary key is the party. A customer quoted
-- from two lists is a question with two answers, and the rule that picks
-- between prices is inside one list by design.

CREATE TABLE party_price_lists (
    party_id      UUID PRIMARY KEY,
    price_list_id UUID NOT NULL REFERENCES price_lists (id) ON DELETE CASCADE,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE party_price_lists IS
    'Which price list a customer is quoted from. party_id is a bare master.parties id - no key across an app boundary.';

COMMENT ON COLUMN party_price_lists.party_id IS
    'master.parties. Bare by ADR 0006 section 8; nothing here resolves it.';
