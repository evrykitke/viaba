-- books 0005: the two roles the sell side needs.
--
-- 0004 mapped what a purchase does. These are what a sale does, added now
-- because Inventory's delivery is about to want them:
--
--   revenue                        what the workspace sells for
--   goods_delivered_not_invoiced   stock gone, customer not yet billed
--
-- The second is the mirror of goods received not invoiced and the one most
-- charts leave out. Without it a delivery posts cost of sales immediately, so a
-- despatch on the 30th and its invoice on the 2nd put the cost in one month and
-- the revenue in the next - and every margin report that straddles a month end
-- is wrong by whatever went out in the last few days of it.
--
-- Same discipline as 0004: ON CONFLICT DO NOTHING, so a workspace that has
-- already chosen keeps its choice, and a role with no matching account is
-- simply absent rather than a guess.

INSERT INTO account_roles (role, account_id)
SELECT 'revenue', a.id
  FROM accounts a
 WHERE a.is_active
   AND a.account_type = 'revenue'
 ORDER BY a.number
 LIMIT 1
    ON CONFLICT DO NOTHING;

-- By number, because the account type alone would pick the first of a dozen
-- other current assets. A workspace that renumbered before this ran gets no
-- mapping, which is the honest outcome - better an unmapped role with a
-- sentence than a delivery accruing into prepaid rent.
INSERT INTO account_roles (role, account_id)
SELECT 'goods_delivered_not_invoiced', a.id
  FROM accounts a
 WHERE a.number = '1270'
   AND a.is_active
    ON CONFLICT DO NOTHING;
