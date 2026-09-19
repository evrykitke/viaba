-- ---------------------------------------------------------------------------
-- 0026: a currency nobody has chosen yet.
--
-- 0010 defaulted the column to USD and 0015 seeded the list from it. Both are
-- applied and frozen, so the correction is here.
-- ---------------------------------------------------------------------------

ALTER TABLE core.organization_profile
    ALTER COLUMN currency_code DROP DEFAULT,
    ALTER COLUMN currency_code DROP NOT NULL;

-- The list row 0015 copied from that default, taken before the value it keys
-- on goes. Only on a profile nobody has written to, and only while nothing has
-- been said about the currency itself; anything already denominated in it
-- raises a foreign key, and the row stays.
DO $do$
BEGIN
    DELETE FROM core.currencies c
     USING core.organization_profile p
     WHERE p.updated_by IS NULL
       AND p.legal_name = ''
       AND c.code = p.currency_code
       AND c.updated_by IS NULL
       AND c.symbol IS NULL;
EXCEPTION WHEN foreign_key_violation THEN
    NULL;
END
$do$;

-- An untouched profile is one nobody has answered for.
UPDATE core.organization_profile
   SET currency_code = NULL
 WHERE updated_by IS NULL
   AND legal_name = '';
