-- books 0007: the index a posted document is found by.
--
-- Posting a sales invoice has stopped being a state change and become an
-- accounting event: it writes a journal, and voiding it writes the reversal.
-- Both need the same lookup - "which journal did this document raise" - and so
-- does every posted invoice that is opened.
--
-- `journals_source` already covers (source_app, source_doc_type, source_doc_id)
-- and would serve. This is narrower on purpose: a workspace posts one journal
-- per document and thousands of documents, so the leading column there is a
-- constant and finding one row means walking the app's whole history under it.
-- Keyed on the document alone, it is one probe.
CREATE INDEX journals_source_doc ON journals (source_doc_id)
    WHERE source_doc_id IS NOT NULL;

-- WHERE THE TWO NEW ROLES ARE MAPPED, AND WHY NOT HERE
--
-- `accounts_receivable` and `tax_payable` are what an invoice debits and
-- credits, and neither had a mapping before now. They are declared in
-- `config/defaults/books.toml` beside the chart they name accounts of, not in
-- this file - see `phonix_db::books::account_role::install_defaults` for the
-- argument. In short: migrations run before the chart is installed, so a
-- statement here would select from an empty table on every new workspace and
-- map nothing.
--
-- What this file does for them is exist. A migration is what makes a workspace
-- outdated, and an outdated workspace is re-migrated - which is the pass that
-- installs the two mappings and grants the permissions that came with them.
