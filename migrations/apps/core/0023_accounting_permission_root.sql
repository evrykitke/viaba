-- ---------------------------------------------------------------------------
-- 0023: Pages.Sales becomes Pages.Accounting.
--
-- The names are not a foreign key to anything, so a grant this build does not
-- define is pruned on load rather than rejected. Without this rewrite every
-- stored grant under the old root would be silently dropped.
--
-- `identity_events` is left alone: it records what a name was at the time.
-- ---------------------------------------------------------------------------

UPDATE core.role_permissions
   SET name = 'Pages.Accounting' || substring(name FROM 12)
 WHERE name = 'Pages.Sales'
    OR name LIKE 'Pages.Sales.%';

UPDATE core.user_permissions
   SET name = 'Pages.Accounting' || substring(name FROM 12)
 WHERE name = 'Pages.Sales'
    OR name LIKE 'Pages.Sales.%';
